//! POST /api/chat — chat completion endpoint.
//!
//! Routes the request to the configured LLM provider. Supports both
//! streaming (SSE) and non-streaming modes. Times out after the
//! provider's configured timeout (default 30s).

use axum::{
    extract::State,
    http::StatusCode,
    response::{
        sse::{Event, Sse},
        IntoResponse, Json,
    },
};
use futures::StreamExt;
use serde_json::json;
use std::convert::Infallible;
use tracing::{error, warn};

use crate::api::ApiState;
use crate::provider;
use provider::{ChatRequest, ChatResponse, ProviderError};

pub async fn chat_handler(
    State(state): State<ApiState>,
    Json(req): Json<ChatRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    // Record metrics
    metrics::counter!(crate::metrics::CHAT_REQUESTS).increment(1);

    // Resolve provider
    let provider = match state.registry.resolve(req.model.as_deref()).await {
        Ok(p) => p,
        Err(e) => {
            return Err(error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                &e.to_string(),
            ));
        }
    };

    // Streaming mode
    if req.stream {
        let timer = std::time::Instant::now();
        match provider.chat_stream(req).await {
            Ok(stream) => {
                let event_stream = stream.map(move |chunk| {
                    // Record latency on first chunk
                    if timer.elapsed().as_secs_f64() < 0.1 {
                        metrics::histogram!(crate::metrics::CHAT_LATENCY)
                            .record(timer.elapsed().as_secs_f64());
                        metrics::counter!(crate::metrics::PROVIDER_REQUESTS).increment(1);
                    }
                    match chunk {
                        Ok(delta) => {
                            let data = serde_json::to_string(&delta).unwrap_or_default();
                            Ok::<_, Infallible>(Event::default().data(data))
                        }
                        Err(e) => {
                            error!(err = %e, "streaming error");
                            let data = serde_json::to_string(&json!({"error": e.to_string()}))
                                .unwrap_or_default();
                            Ok(Event::default().data(data))
                        }
                    }
                });

                Ok(Sse::new(event_stream).into_response())
            }
            Err(e) => {
                metrics::counter!(crate::metrics::PROVIDER_ERRORS).increment(1);
                Err(error_response(
                    StatusCode::SERVICE_UNAVAILABLE,
                    &e.to_string(),
                ))
            }
        }
    } else {
        // Non-streaming mode. When the caller didn't pin a model, route
        // cost-awarely and escalate to a stronger provider on a transient
        // failure; otherwise use the model the caller chose.
        let timer = std::time::Instant::now();
        let result = if req.model.is_none() {
            routed_chat_with_escalation(&state, req).await
        } else {
            provider.chat(req).await
        };
        match result {
            Ok(resp) => {
                metrics::histogram!(crate::metrics::CHAT_LATENCY)
                    .record(timer.elapsed().as_secs_f64());
                metrics::counter!(crate::metrics::PROVIDER_REQUESTS).increment(1);
                Ok(Json(resp).into_response())
            }
            Err(e) => {
                metrics::counter!(crate::metrics::PROVIDER_ERRORS).increment(1);
                warn!(err = %e, "chat request failed");
                let (status, msg) = map_provider_error(&e);
                Err(error_response(status, &msg))
            }
        }
    }
}

/// Route the request cost-awarely; on a transient failure of the chosen
/// provider, retry once with the escalation (stronger) provider. Falls back to
/// round-robin resolution when no provider is capable.
async fn routed_chat_with_escalation(
    state: &ApiState,
    req: ChatRequest,
) -> Result<ChatResponse, ProviderError> {
    let query = last_user_text(&req);
    let configs = state.registry.provider_configs().await;

    let Some((primary, escalation)) = crate::routing::route_with_escalation(&configs, &query, &[])
    else {
        // No routing match → round-robin.
        let p = state.registry.resolve(req.model.as_deref()).await?;
        return p.chat(req).await;
    };

    let primary_provider = state
        .registry
        .get(&primary)
        .await
        .ok_or_else(|| ProviderError::NotFound(primary.clone()))?;

    match primary_provider.chat(req.clone()).await {
        Ok(resp) => Ok(resp),
        Err(e) => {
            if is_transient(&e) {
                if let Some(esc_name) = escalation {
                    if let Some(esc) = state.registry.get(&esc_name).await {
                        warn!(from = %primary, to = %esc_name, err = %e, "escalating to stronger provider");
                        metrics::counter!(crate::metrics::PROVIDER_REQUESTS).increment(1);
                        return esc.chat(req).await;
                    }
                }
            }
            Err(e)
        }
    }
}

/// The text of the most recent user message — the routing query.
fn last_user_text(req: &ChatRequest) -> String {
    req.messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.clone())
        .unwrap_or_default()
}

/// Whether a provider error is worth escalating (transient / upstream fault,
/// not a client error).
fn is_transient(e: &ProviderError) -> bool {
    match e {
        ProviderError::Timeout(_) | ProviderError::CircuitOpen(_) | ProviderError::Http(_) => true,
        ProviderError::Upstream { status, .. } => *status >= 500,
        _ => false,
    }
}

fn map_provider_error(e: &ProviderError) -> (StatusCode, String) {
    match e {
        ProviderError::NotFound(_) => (StatusCode::NOT_FOUND, e.to_string()),
        ProviderError::ModelNotAvailable(_) => (StatusCode::BAD_REQUEST, e.to_string()),
        ProviderError::Timeout(_) => (StatusCode::GATEWAY_TIMEOUT, e.to_string()),
        ProviderError::CircuitOpen(_) => (StatusCode::SERVICE_UNAVAILABLE, e.to_string()),
        ProviderError::Upstream { status: s, .. } if *s >= 500 => {
            (StatusCode::BAD_GATEWAY, e.to_string())
        }
        ProviderError::Upstream { .. } => (StatusCode::BAD_REQUEST, e.to_string()),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

fn error_response(status: StatusCode, msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(json!({"error": msg})))
}
