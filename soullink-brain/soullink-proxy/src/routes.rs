//! Route definitions — maps URL prefixes to backend services.

use crate::config::ProxyConfig;
use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use std::sync::Arc;

/// Application state shared across handlers.
#[derive(Clone)]
pub struct AppState {
    pub config: ProxyConfig,
    /// HTTP client for proxying requests.
    pub client: reqwest::Client,
}

impl AppState {
    pub fn new(config: ProxyConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("failed to create HTTP client");
        Self { config, client }
    }

    /// Find the backend URL for a given path.
    pub fn find_backend(&self, path: &str) -> Option<(String, String)> {
        // Sort by prefix length (longest match first)
        let mut backends = self.config.backends.clone();
        backends.sort_by(|a, b| b.prefix.len().cmp(&a.prefix.len()));

        for backend in backends {
            if path.starts_with(&backend.prefix) {
                let stripped = path.strip_prefix(&backend.prefix).unwrap_or("/");
                let target = format!("{}{}", backend.url.trim_end_matches('/'), stripped);
                return Some((backend.prefix.clone(), target));
            }
        }
        None
    }
}

/// Health check endpoint — reports status of all backends.
pub async fn health_check(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut statuses = Vec::new();

    for backend in &state.config.backends {
        let health_url = format!("{}{}", backend.url.trim_end_matches('/'), backend.health);
        let status = match state.client.get(&health_url).send().await {
            Ok(resp) => resp.status().as_u16(),
            Err(_) => 503,
        };
        statuses.push(serde_json::json!({
            "prefix": backend.prefix,
            "url": backend.url,
            "health": status,
        }));
    }

    let all_healthy = statuses.iter().all(|s| s["health"].as_u64() == Some(200));
    let status_code = if all_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status_code,
        axum::Json(serde_json::json!({
            "healthy": all_healthy,
            "backends": statuses,
        })),
    )
        .into_response()
}

/// Proxy handler — forwards requests to the appropriate backend.
pub async fn proxy_handler(State(state): State<Arc<AppState>>, req: Request<Body>) -> Response {
    let path = req.uri().path().to_string();

    let (prefix, target_url) = match state.find_backend(&path) {
        Some(result) => result,
        None => {
            return (
                StatusCode::NOT_FOUND,
                format!("No backend for path: {}", path),
            )
                .into_response();
        }
    };

    // Build the proxied request
    let method = req.method().clone();
    let body_bytes = match axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024).await {
        Ok(bytes) => bytes,
        Err(_) => return (StatusCode::BAD_REQUEST, "Failed to read body").into_response(),
    };

    let mut builder = match method {
        axum::http::Method::GET => state.client.get(&target_url),
        axum::http::Method::POST => state.client.post(&target_url),
        axum::http::Method::PUT => state.client.put(&target_url),
        axum::http::Method::DELETE => state.client.delete(&target_url),
        axum::http::Method::PATCH => state.client.patch(&target_url),
        _ => state.client.get(&target_url),
    };

    builder = builder
        .body(body_bytes)
        .header("X-Proxied-By", "soullink-proxy/0.1")
        .header("X-Original-Path", &path);

    match builder.send().await {
        Ok(resp) => {
            let status = resp.status();
            let headers = resp.headers().clone();
            let body = resp.bytes().await.unwrap_or_default();

            let mut builder = axum::response::Response::builder().status(status);
            for (name, value) in headers.iter() {
                // En-têtes hop-by-hop recalculés par axum/hyper.
                if matches!(
                    name.as_str(),
                    "connection" | "transfer-encoding" | "content-length"
                ) {
                    continue;
                }
                builder = builder.header(name, value);
            }
            builder.body(Body::from(body.to_vec())).unwrap_or_else(|_| {
                axum::response::Response::builder()
                    .status(StatusCode::BAD_GATEWAY)
                    .body(Body::from("proxy error"))
                    .unwrap()
            })
        }
        Err(e) => {
            tracing::warn!(prefix = %prefix, target = %target_url, error = %e, "Proxy request failed");
            (
                StatusCode::BAD_GATEWAY,
                format!("Backend unavailable: {}", e),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_backend_mesh() {
        let config = ProxyConfig::default();
        let state = AppState::new(config);
        let (prefix, url) = state.find_backend("/api/mesh/status").unwrap();
        assert_eq!(prefix, "/api/mesh");
        assert!(url.starts_with("http://127.0.0.1:9020"));
    }

    #[test]
    fn find_backend_rag() {
        let config = ProxyConfig::default();
        let state = AppState::new(config);
        let (prefix, url) = state.find_backend("/api/rag/search").unwrap();
        assert_eq!(prefix, "/api/rag");
        assert!(url.contains("9020"));
    }

    #[test]
    fn find_backend_autonomy() {
        let config = ProxyConfig::default();
        let state = AppState::new(config);
        let (prefix, url) = state.find_backend("/api/autonomy/status").unwrap();
        assert_eq!(prefix, "/api/autonomy");
        assert!(url.contains("9046"));
    }

    #[test]
    fn find_backend_unknown_returns_none() {
        let config = ProxyConfig::default();
        let state = AppState::new(config);
        assert!(state.find_backend("/unknown/path").is_none());
    }

    #[test]
    fn longest_prefix_match() {
        let config = ProxyConfig::default();
        let state = AppState::new(config);
        // /api/rag should match /api/rag, not /api/mesh
        let (prefix, _) = state.find_backend("/api/rag/health").unwrap();
        assert_eq!(prefix, "/api/rag");
    }
}
