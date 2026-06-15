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
};
use serde_json::Value;
use std::collections::HashMap;
use tracing::{info, warn};

use crate::api::ApiState;
use crate::channels::discord::DiscordChannel;
use crate::channels::slack::SlackChannel;
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
    body: axum::body::Bytes,
) -> impl IntoResponse {
    info!(provider = %provider, "webhook received");

    // Per-provider verification + handshake handling (signatures, URL/PING
    // challenges) against the *raw* body, before trusting/forwarding anything.
    match provider.as_str() {
        "whatsapp" => {
            let sig = header(&headers, "x-hub-signature-256");
            let app_secret = std::env::var("WHATSAPP_APP_SECRET").ok();
            if !signature_ok(app_secret.as_deref(), &body, sig) {
                warn!("WhatsApp webhook signature verification failed");
                return (StatusCode::UNAUTHORIZED, "invalid signature".to_string());
            }
        }
        "slack" => {
            if let Prelude::Respond(status, text) = slack_prelude(
                std::env::var("SLACK_SIGNING_SECRET").ok().as_deref(),
                &headers,
                &body,
            ) {
                return (status, text);
            }
        }
        "discord" => {
            if let Prelude::Respond(status, text) = discord_prelude(
                std::env::var("DISCORD_PUBLIC_KEY").ok().as_deref(),
                &headers,
                &body,
            ) {
                return (status, text);
            }
        }
        _ => {}
    }

    let payload: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);

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
            "body": payload,
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

/// Verify a WhatsApp `X-Hub-Signature-256` against the raw body with the given
/// app secret. Lenient (`true`) when `app_secret` is `None`.
fn signature_ok(app_secret: Option<&str>, raw: &[u8], sig_header: &str) -> bool {
    let channel = WhatsAppChannel {
        app_secret: app_secret.map(String::from),
        ..WhatsAppChannel::new()
    };
    channel.verify_signature(raw, sig_header)
}

/// The outcome of a provider's pre-forward handling.
#[derive(Debug, PartialEq, Eq)]
enum Prelude {
    /// Return this response immediately (a challenge, a PONG, or a 401).
    Respond(StatusCode, String),
    /// Verified (or no verification configured) — proceed to forward.
    Proceed,
}

/// A header value as a `&str` (empty when absent or non-ASCII).
fn header<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

/// Slack: verify the request signature, then answer the URL-verification
/// handshake. `signing_secret == None` is lenient.
fn slack_prelude(signing_secret: Option<&str>, headers: &HeaderMap, body: &[u8]) -> Prelude {
    let ch = SlackChannel {
        signing_secret: signing_secret.map(String::from),
        ..SlackChannel::new()
    };
    let ts = header(headers, "x-slack-request-timestamp");
    let sig = header(headers, "x-slack-signature");
    if !ch.verify_signature(ts, body, sig) {
        warn!("Slack webhook signature verification failed");
        return Prelude::Respond(StatusCode::UNAUTHORIZED, "invalid signature".into());
    }
    let payload: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    if let Some(challenge) = SlackChannel::url_verification(&payload) {
        return Prelude::Respond(StatusCode::OK, challenge);
    }
    Prelude::Proceed
}

/// Discord: verify the Ed25519 signature, then answer the PING handshake.
/// `public_key == None` is lenient.
fn discord_prelude(public_key: Option<&str>, headers: &HeaderMap, body: &[u8]) -> Prelude {
    let ch = DiscordChannel {
        public_key: public_key.map(String::from),
        ..DiscordChannel::new()
    };
    let ts = header(headers, "x-signature-timestamp");
    let sig = header(headers, "x-signature-ed25519");
    if !ch.verify_signature(ts, body, sig) {
        warn!("Discord webhook signature verification failed");
        return Prelude::Respond(StatusCode::UNAUTHORIZED, "invalid signature".into());
    }
    let payload: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    if DiscordChannel::is_ping(&payload) {
        return Prelude::Respond(StatusCode::OK, DiscordChannel::pong().to_string());
    }
    Prelude::Proceed
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
    fn signature_ok_is_lenient_without_secret() {
        // No configured app secret → accept (matches existing ingress posture).
        assert!(signature_ok(None, b"{}", "sha256=anything"));
    }

    #[test]
    fn signature_ok_validates_hmac_when_secret_set() {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        let body = br#"{"object":"whatsapp_business_account"}"#;
        let mut mac = Hmac::<Sha256>::new_from_slice(b"top-secret").unwrap();
        mac.update(body);
        let hex: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();

        assert!(signature_ok(
            Some("top-secret"),
            body,
            &format!("sha256={hex}")
        ));
        assert!(!signature_ok(Some("top-secret"), body, "sha256=deadbeef"));
        assert!(!signature_ok(
            Some("top-secret"),
            b"tampered",
            &format!("sha256={hex}")
        ));
    }

    #[test]
    fn slack_prelude_echoes_url_verification() {
        let headers = HeaderMap::new();
        let body = br#"{"type":"url_verification","challenge":"xyz"}"#;
        // No signing secret → lenient, challenge echoed.
        assert_eq!(
            slack_prelude(None, &headers, body),
            Prelude::Respond(StatusCode::OK, "xyz".to_string())
        );
    }

    #[test]
    fn slack_prelude_proceeds_on_event() {
        let headers = HeaderMap::new();
        let body = br#"{"type":"event_callback","event":{"type":"message"}}"#;
        assert_eq!(slack_prelude(None, &headers, body), Prelude::Proceed);
    }

    #[test]
    fn slack_prelude_rejects_bad_signature() {
        let headers = HeaderMap::new(); // no signature headers
        let body = br#"{"type":"event_callback"}"#;
        // With a secret configured but no/!matching signature → 401.
        match slack_prelude(Some("secret"), &headers, body) {
            Prelude::Respond(status, _) => assert_eq!(status, StatusCode::UNAUTHORIZED),
            Prelude::Proceed => panic!("expected 401"),
        }
    }

    #[test]
    fn discord_prelude_answers_ping() {
        let headers = HeaderMap::new();
        let body = br#"{"type":1}"#;
        match discord_prelude(None, &headers, body) {
            Prelude::Respond(status, text) => {
                assert_eq!(status, StatusCode::OK);
                assert!(text.contains("\"type\":1"));
            }
            Prelude::Proceed => panic!("expected PONG"),
        }
    }

    #[test]
    fn discord_prelude_proceeds_on_command() {
        let headers = HeaderMap::new();
        let body = br#"{"type":2,"data":{"name":"ask"}}"#;
        assert_eq!(discord_prelude(None, &headers, body), Prelude::Proceed);
    }

    #[test]
    fn discord_prelude_rejects_bad_signature() {
        let headers = HeaderMap::new(); // no signature headers
        let body = br#"{"type":1}"#;
        match discord_prelude(Some("abcdef"), &headers, body) {
            Prelude::Respond(status, _) => assert_eq!(status, StatusCode::UNAUTHORIZED),
            Prelude::Proceed => panic!("expected 401"),
        }
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
