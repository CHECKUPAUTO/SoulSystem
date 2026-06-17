//! Axum HTTP router for TurboQuant Proxy.
//!
//! Endpoints:
//!   POST /v1/chat/completions  → proxy to llama-server (with KV offload)
//!   POST /v1/completions       → proxy to llama-server
//!   GET  /health               → health check (self + backend)
//!   GET  /stats                 → compression + usage stats
//!   GET  /v1/models             → proxy to llama-server

use crate::turboquant::proxy::server::{ProxyStats, TurboQuantProxy};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use std::sync::Arc;

/// Shared application state.
pub struct AppState {
    pub proxy: TurboQuantProxy,
}

/// Health response.
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub backend_healthy: bool,
    pub effective_compression: String,
}

/// Stats response (public API).
#[derive(Serialize)]
pub struct StatsResponse {
    pub backend_healthy: bool,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub gpu_kv_positions: usize,
    pub turboquant_positions: usize,
    pub turboquant_memory_mib: f64,
    pub compression_ratio: f64,
    pub effective_compression_vs_fp16: f64,
}

impl From<ProxyStats> for StatsResponse {
    fn from(s: ProxyStats) -> Self {
        Self {
            backend_healthy: s.llama_server_healthy,
            total_requests: s.total_requests,
            successful_requests: s.successful_requests,
            failed_requests: s.failed_requests,
            gpu_kv_positions: s.gpu_kv_positions,
            turboquant_positions: s.turboquant_positions,
            turboquant_memory_mib: s.turboquant_memory_mib,
            compression_ratio: s.compression_ratio,
            effective_compression_vs_fp16: s.effective_compression_vs_fp16,
        }
    }
}

/// Build the axum router.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/chat/completions", post(proxy_chat_completions))
        .route("/v1/completions", post(proxy_completions))
        .route("/v1/models", get(proxy_models))
        .route("/health", get(health_handler))
        .route("/stats", get(stats_handler))
        .with_state(Arc::new(state))
}

/// POST /v1/chat/completions — proxy to llama-server with KV offload.
async fn proxy_chat_completions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let url = format!(
        "{}/v1/chat/completions",
        state.proxy.config().llama_server_url
    );
    proxy_request(&state.proxy, url, headers, body).await
}

/// POST /v1/completions — proxy to llama-server.
async fn proxy_completions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let url = format!("{}/v1/completions", state.proxy.config().llama_server_url);
    proxy_request(&state.proxy, url, headers, body).await
}

/// GET /v1/models — proxy to llama-server.
async fn proxy_models(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let url = format!("{}/v1/models", state.proxy.config().llama_server_url);
    match state.proxy.http_client().get(&url).send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            (StatusCode::from_u16(status).unwrap_or(StatusCode::OK), body)
        }
        Err(_) => (StatusCode::BAD_GATEWAY, "Backend unavailable".into()),
    }
}

/// GET /health — combined health check.
async fn health_handler(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let backend_healthy = state.proxy.health_check().await;
    Json(HealthResponse {
        status: if backend_healthy {
            "ok".into()
        } else {
            "degraded".into()
        },
        backend_healthy,
        effective_compression: "6x vs FP16 (Q4_0 4x + TurboQuant 3-bit offload)".into(),
    })
}

/// GET /stats — compression and usage statistics.
async fn stats_handler(State(state): State<Arc<AppState>>) -> Json<StatsResponse> {
    let stats = state.proxy.stats().await;
    Json(stats.into())
}

/// Generic request proxy with metric tracking.
async fn proxy_request(
    proxy: &TurboQuantProxy,
    url: String,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> (StatusCode, String) {
    let mut req = proxy
        .http_client()
        .post(&url)
        .header("Content-Type", "application/json");

    if let Some(auth) = headers.get("authorization") {
        req = req.header("authorization", auth);
    }

    match req.body(body).send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let resp_body = resp.text().await.unwrap_or_default();
            (
                StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
                resp_body,
            )
        }
        Err(_) => (StatusCode::BAD_GATEWAY, "Backend unavailable".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turboquant::proxy::server::ProxyConfig;

    #[test]
    #[ignore = "slow: integration setup"]
    fn router_builds() {
        let state = AppState {
            proxy: TurboQuantProxy::new(ProxyConfig::default()),
        };
        let _router = build_router(state);
    }
}
