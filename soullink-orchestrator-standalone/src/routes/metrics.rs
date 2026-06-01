//! Route: Metrics

use crate::state::AppState;
use crate::utils::helpers::format_metrics;
use axum::{extract::State, response::Response};
use axum::http::{header, StatusCode};

/// Route GET /metrics
pub async fn route_metrics(State(state): State<AppState>) -> Response {
    let metrics = format_metrics(
        state.get_query_count(),
        state.get_spawn_count(),
        state.brains_count(),
    );

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(metrics.into())
        .unwrap()
}
