//! POST /api/tools/call — invoke a tool via the LLM provider.
//!
//! Routes the tool invocation to the configured provider. For Ollama,
//! tools are passed through as part of the chat. For OpenAI, function
//! calling is used.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde_json::json;
use tracing::warn;

use crate::api::ApiState;
use crate::provider;
use provider::{ToolCallRequest, ToolCallResponse};

pub async fn tools_call_handler(
    State(state): State<ApiState>,
    Json(req): Json<ToolCallRequest>,
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

    // Tool calls are done via a chat request with a tool message
    let chat_req = provider::ChatRequest {
        model: req.model,
        messages: vec![provider::ChatMessage {
            role: "user".into(),
            content: format!("Execute the tool `{}` with args: {}", req.tool, req.args),
        }],
        stream: false,
        temperature: 0.1,
        max_tokens: 2048,
    };

    let timer = std::time::Instant::now();
    match provider.chat(chat_req).await {
        Ok(resp) => {
            metrics::histogram!(crate::metrics::PROVIDER_LATENCY)
                .record(timer.elapsed().as_secs_f64());
            metrics::counter!(crate::metrics::PROVIDER_REQUESTS).increment(1);
            Ok(Json(ToolCallResponse {
                result: json!({
                    "content": resp.message.content,
                    "tool": req.tool,
                }),
            })
            .into_response())
        }
        Err(e) => {
            metrics::counter!(crate::metrics::PROVIDER_ERRORS).increment(1);
            warn!(err = %e, "tool call failed");
            Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &e.to_string(),
            ))
        }
    }
}

fn error_response(status: StatusCode, msg: &str) -> (StatusCode, Json<serde_json::value::Value>) {
    (status, Json(json!({"error": msg})))
}
