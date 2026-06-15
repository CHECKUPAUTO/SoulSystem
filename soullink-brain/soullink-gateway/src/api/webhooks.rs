//! POST /api/webhooks/{provider} — webhook ingress endpoint.
//!
//! Accepts webhooks from GitHub, GitLab, Slack, and other services.
//! Routes the incoming payload to the orchestrator at `/api/mesh/webhook`.
//!
//! The provider name in the URL is used for routing; each provider may
//! have a different payload format.

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde_json::Value;
use std::collections::HashMap;
use tracing::{info, warn};

use crate::api::ApiState;
use crate::channels::whatsapp::WhatsAppChannel;

/// GET handler for the webhook subscription handshake.
///
/// WhatsApp (and other Meta products) register a webhook by issuing a GET with
/// `hub.mode=subscribe`, `hub.verify_token`, and `hub.challenge`; the endpoint
/// must echo the challenge when the token matches. The expected token is read
/// from `WHATSAPP_VERIFY_TOKEN`.
pub async fn webhook_verify(
    Path(provider): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    if provider != "whatsapp" {
        return (
            StatusCode::NOT_FOUND,
            format!("no subscription handshake for provider '{provider}'"),
        );
    }
    let channel = WhatsAppChannel {
        verify_token: std::env::var("WHATSAPP_VERIFY_TOKEN").ok(),
        ..WhatsAppChannel::new()
    };
    let mode = params.get("hub.mode").map(String::as_str).unwrap_or("");
    let token = params
        .get("hub.verify_token")
        .map(String::as_str)
        .unwrap_or("");
    let challenge = params
        .get("hub.challenge")
        .map(String::as_str)
        .unwrap_or("");

    match channel.verify_subscription(mode, token, challenge) {
        Some(echo) => {
            info!("WhatsApp webhook subscription verified");
            (StatusCode::OK, echo)
        }
        None => {
            warn!("WhatsApp webhook subscription verification failed");
            (StatusCode::FORBIDDEN, "verification failed".to_string())
        }
    }
}

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

    #[tokio::test]
    async fn webhook_verify_rejects_unknown_provider() {
        let resp = webhook_verify(Path("telegram".into()), Query(HashMap::new()))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn webhook_verify_forbidden_when_token_mismatch() {
        // With no configured verify token (env unset), any handshake fails.
        let mut params = HashMap::new();
        params.insert("hub.mode".to_string(), "subscribe".to_string());
        params.insert("hub.verify_token".to_string(), "whatever".to_string());
        params.insert("hub.challenge".to_string(), "1234".to_string());
        let resp = webhook_verify(Path("whatsapp".into()), Query(params))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

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
