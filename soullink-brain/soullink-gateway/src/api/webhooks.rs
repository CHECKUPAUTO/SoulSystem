//! POST /api/webhooks/{provider} — webhook ingress endpoint.
//!
//! Accepts webhooks from GitHub, GitLab, Slack, and other services.
//! Routes the incoming payload to the orchestrator at `/api/mesh/webhook`.
//!
//! The provider name in the URL is used for routing; each provider may
//! have a different payload format.

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde_json::Value;
use tracing::{info, warn};

use crate::api::ApiState;

pub async fn webhook_handler(
    State(_state): State<ApiState>,
    Path(provider): Path<String>,
    headers: HeaderMap,
    body: Json<Value>,
) -> impl IntoResponse {
    info!(provider = %provider, "webhook received");

    // Forward to orchestrator
    let orch_url = "http://127.0.0.1:9020".to_string();
    let url = format!("{}/api/mesh/webhook", orch_url.trim_end_matches('/'));

    let client = soullink_core::http::shared_client();
    let mut req = client
        .post(&url)
        .timeout(std::time::Duration::from_secs(10))
        .json(&serde_json::json!({
            "provider": provider,
            "headers": serialize_headers(&headers),
            "body": body.0,
        }));

    // Forward signature headers if present
    if let Some(sig) = headers.get("x-hub-signature-256") {
        if let Ok(s) = sig.to_str() {
            req = req.header("x-hub-signature-256", s);
        }
    }

    match req.send().await {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            info!(provider = %provider, status = status.as_u16(), "webhook forwarded");
            (status, body)
        }
        Err(e) => {
            warn!(provider = %provider, err = %e, "webhook forward failed");
            (
                StatusCode::BAD_GATEWAY,
                format!("orchestrator unreachable: {e}"),
            )
        }
    }
}

fn serialize_headers(headers: &HeaderMap) -> Vec<serde_json::Value> {
    headers
        .iter()
        .filter(|(k, _)| {
            let name = k.as_str();
            !name.eq_ignore_ascii_case("host") && !name.eq_ignore_ascii_case("content-length")
        })
        .map(|(k, v)| {
            serde_json::json!({
                "name": k.as_str(),
                "value": v.to_str().unwrap_or(""),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_headers_filters_sensitive() {
        let mut headers = HeaderMap::new();
        headers.insert("x-hub-signature-256", "sha256=abc123".parse().unwrap());
        headers.insert("host", "example.com".parse().unwrap());
        headers.insert("content-type", "application/json".parse().unwrap());

        let result = serialize_headers(&headers);
        // host should be filtered out
        let names: Vec<_> = result.iter().filter_map(|h| h["name"].as_str()).collect();
        assert!(!names.contains(&"host"));
        assert!(names.contains(&"x-hub-signature-256"));
    }
}
