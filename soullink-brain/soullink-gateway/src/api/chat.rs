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
use provider::{ChatRequest, ProviderError};

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
        // Non-streaming mode
        let timer = std::time::Instant::now();
        match provider.chat(req).await {
            Ok(resp) => {
                metrics::histogram!(crate::metrics::CHAT_LATENCY)
                    .record(timer.elapsed().as_secs_f64());
                metrics::counter!(crate::metrics::PROVIDER_REQUESTS).increment(1);
                Ok(Json(resp).into_response())
            }
            Err(e) => {
                metrics::counter!(crate::metrics::PROVIDER_ERRORS).increment(1);
                warn!(err = %e, "chat request failed");

                let (status, msg) = match &e {
                    ProviderError::NotFound(_) => (StatusCode::NOT_FOUND, e.to_string()),
                    ProviderError::ModelNotAvailable(_) => (StatusCode::BAD_REQUEST, e.to_string()),
                    ProviderError::Timeout(_) => (StatusCode::GATEWAY_TIMEOUT, e.to_string()),
                    ProviderError::CircuitOpen(_) => {
                        (StatusCode::SERVICE_UNAVAILABLE, e.to_string())
                    }
                    ProviderError::Upstream { status: s, body: _ } => {
                        if *s >= 500 {
                            (StatusCode::BAD_GATEWAY, e.to_string())
                        } else {
                            (StatusCode::BAD_REQUEST, e.to_string())
                        }
                    }
                    _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
                };
                Err(error_response(status, &msg))
            }
        }
    }
}

fn error_response(status: StatusCode, msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(json!({"error": msg})))
}
