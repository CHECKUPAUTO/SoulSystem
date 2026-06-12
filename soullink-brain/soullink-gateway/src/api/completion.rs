//! POST /api/completion — simple text completion.
//!
//! Non-streaming completion via the resolved provider. Simpler than
//! /api/chat — single prompt → text response.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use tracing::warn;

use crate::api::ApiState;
use crate::provider;
use provider::{CompletionRequest, ProviderError};

pub async fn completion_handler(
    State(state): State<ApiState>,
    Json(req): Json<CompletionRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let provider = match state.registry.resolve(req.model.as_deref()).await {
        Ok(p) => p,
        Err(e) => {
            return Err(error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                &e.to_string(),
            ));
        }
    };

    let timer = std::time::Instant::now();
    match provider.complete(req).await {
        Ok(resp) => {
            metrics::histogram!(crate::metrics::PROVIDER_LATENCY)
                .record(timer.elapsed().as_secs_f64());
            metrics::counter!(crate::metrics::PROVIDER_REQUESTS).increment(1);
            Ok(Json(resp).into_response())
        }
        Err(e) => {
            metrics::counter!(crate::metrics::PROVIDER_ERRORS).increment(1);
            warn!(err = %e, "completion request failed");
            let status = match &e {
                ProviderError::NotFound(_) => StatusCode::NOT_FOUND,
                ProviderError::Timeout(_) => StatusCode::GATEWAY_TIMEOUT,
                ProviderError::CircuitOpen(_) => StatusCode::SERVICE_UNAVAILABLE,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            Err(error_response(status, &e.to_string()))
        }
    }
}

fn error_response(status: StatusCode, msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({"error": msg})))
}
