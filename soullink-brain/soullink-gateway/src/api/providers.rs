//! GET /api/providers — list configured LLM providers.
//!
//! Returns provider names, types, base URLs, available models, and
//! circuit breaker status.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

use crate::api::ApiState;

pub async fn list_providers(
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let providers = state.registry.list_providers().await;
    Ok(Json(json!({ "providers": providers })))
}
