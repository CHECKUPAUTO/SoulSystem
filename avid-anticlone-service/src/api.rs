use std::sync::Arc;

use axum::{
    extract::{Json, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::json;
use tracing::error;

use crate::corpus::Corpus;

#[derive(Clone)]
pub(crate) struct AppState {
    pub corpus: Arc<Corpus>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CheckRequest {
    pub submission: crate::corpus::Submission,
    #[serde(default = "default_threshold")]
    pub threshold: f32,
}

const fn default_threshold() -> f32 {
    0.75
}

#[derive(Debug, Deserialize)]
pub(crate) struct SubmitRequest {
    pub label: Option<String>,
    pub submission: crate::corpus::Submission,
}

pub(crate) fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { (StatusCode::OK, "ok") }))
        .route("/v1/check", post(check_handler))
        .route("/v1/submit", post(submit_handler))
        .layer(tower_http::limit::RequestBodyLimitLayer::new(1024 * 1024))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}

async fn check_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CheckRequest>,
) -> impl IntoResponse {
    let Some(tenant_id) = crate::auth::authenticate(&headers, &state.corpus) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "invalid api key"})),
        )
            .into_response();
    };
    match state
        .corpus
        .check(tenant_id, &req.submission, req.threshold)
    {
        Ok(report) => (
            StatusCode::OK,
            Json(json!({
                "score": report.score,
                "verdict": report.verdict,
                "closest_id": report.closest_id,
            })),
        )
            .into_response(),
        Err(e) => {
            error!(error = %e, "check failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal"})),
            )
                .into_response()
        }
    }
}

async fn submit_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<SubmitRequest>,
) -> impl IntoResponse {
    let Some(tenant_id) = crate::auth::authenticate(&headers, &state.corpus) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "invalid api key"})),
        )
            .into_response();
    };
    match state
        .corpus
        .submit(tenant_id, req.label.as_deref(), &req.submission)
    {
        Ok(id) => (StatusCode::CREATED, Json(json!({"id": id}))).into_response(),
        Err(e) => {
            error!(error = %e, "submit failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal"})),
            )
                .into_response()
        }
    }
}
