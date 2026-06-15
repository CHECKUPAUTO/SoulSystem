//! Slack channel — Slack Web API (outbound) + Events API (inbound).
//!
//! Outbound messages go to `chat.postMessage` with a bot token. Inbound events
//! arrive as Events API POSTs; this module answers the one-time `url_verification`
//! challenge, verifies the `X-Slack-Signature` request signature, and parses
//! `event_callback` message events (skipping the bot's own echoes).
//!
//! Reference: <https://api.slack.com/apis/connections/events-api>

use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use tracing::{info, warn};

const DEFAULT_API_BASE: &str = "https://slack.com/api";

/// A single inbound Slack message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundMessage {
    /// Slack user ID of the sender (`U...`).
    pub user: String,
    /// Message text.
    pub text: String,
    /// Channel ID the message was posted in (`C...`/`D...`).
    pub channel: String,
    /// Message timestamp (`ts`), Slack's per-channel message ID.
    pub ts: String,
}

pub struct SlackChannel {
    pub enabled: bool,
    /// Web API base, e.g. `https://slack.com/api`.
    pub api_base: String,
    /// Bot user OAuth token (`xoxb-...`).
    pub bot_token: Option<String>,
    /// App signing secret, used to verify the `X-Slack-Signature` header.
    pub signing_secret: Option<String>,
}

impl Default for SlackChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl SlackChannel {
    /// A disabled channel with Web API defaults — fill in credentials to enable.
    pub fn new() -> Self {
        Self {
            enabled: false,
            api_base: DEFAULT_API_BASE.to_string(),
            bot_token: None,
            signing_secret: None,
        }
    }

    /// Build an enabled channel from a bot token.
    pub fn configured(bot_token: impl Into<String>) -> Self {
        Self {
            enabled: true,
            bot_token: Some(bot_token.into()),
            ..Self::new()
        }
    }

    /// Post a message to a channel via `chat.postMessage`.
    pub async fn send_message(&self, channel: &str, text: &str) -> Result<(), String> {
        if !self.enabled {
            return Err("Slack channel is disabled (no credentials configured)".into());
        }
        let token = self
            .bot_token
            .as_ref()
            .ok_or("Slack bot_token not configured")?;
        let url = format!("{}/chat.postMessage", self.api_base.trim_end_matches('/'));

        let client = soullink_core::http::shared_client();
        let resp = client
            .post(&url)
            .bearer_auth(token)
            .json(&serde_json::json!({ "channel": channel, "text": text }))
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| format!("slack send failed: {e}"))?;

        let status = resp.status();
        let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
        // Slack returns HTTP 200 with `{"ok": false, "error": "..."}` on logical
        // failures, so check the body, not just the status.
        if status.is_success() && body["ok"].as_bool() == Some(true) {
            info!(channel = %channel, "Slack message sent");
            Ok(())
        } else {
            let err = body["error"].as_str().unwrap_or("unknown");
            warn!(status = status.as_u16(), error = %err, "Slack send error");
            Err(format!("slack error: {err}"))
        }
    }

    /// Answer the one-time Events API URL-verification handshake: when the
    /// payload is `{"type": "url_verification", "challenge": "..."}`, return the
    /// challenge to echo back.
    pub fn url_verification(payload: &serde_json::Value) -> Option<String> {
        if payload["type"] == "url_verification" {
            payload["challenge"].as_str().map(str::to_string)
        } else {
            None
        }
    }

    /// Verify the `X-Slack-Signature` (`v0=<hex>`) against the raw body and the
    /// request timestamp, per Slack's signing scheme. Lenient (`true`) when no
    /// signing secret is configured.
    pub fn verify_signature(
        &self,
        timestamp: &str,
        raw_body: &[u8],
        signature_header: &str,
    ) -> bool {
        let Some(secret) = self.signing_secret.as_ref() else {
            return true;
        };
        let body = std::str::from_utf8(raw_body).unwrap_or("");
        let basestring = format!("v0:{timestamp}:{body}");
        let expected = format!("v0={}", hmac_sha256_hex(secret, &basestring));
        expected
            .as_bytes()
            .ct_eq(signature_header.as_bytes())
            .into()
    }

    /// Parse inbound user messages from an `event_callback` payload, skipping
    /// the bot's own messages (which carry a `bot_id`) and message subtypes
    /// (edits, joins, etc.).
    pub fn parse_inbound(payload: &serde_json::Value) -> Vec<InboundMessage> {
        if payload["type"] != "event_callback" {
            return Vec::new();
        }
        let event = &payload["event"];
        if event["type"] != "message" {
            return Vec::new();
        }
        // Skip bot echoes and non-plain message events.
        if event.get("bot_id").is_some() || event.get("subtype").is_some() {
            return Vec::new();
        }
        let (Some(user), Some(text), Some(channel), Some(ts)) = (
            event["user"].as_str(),
            event["text"].as_str(),
            event["channel"].as_str(),
            event["ts"].as_str(),
        ) else {
            return Vec::new();
        };
        vec![InboundMessage {
            user: user.to_string(),
            text: text.to_string(),
            channel: channel.to_string(),
            ts: ts.to_string(),
        }]
    }
}

/// Hex-encoded HMAC-SHA256 of `msg` under `secret`.
fn hmac_sha256_hex(secret: &str, msg: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(msg.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn url_verification_echoes_challenge() {
        let payload = json!({"type": "url_verification", "challenge": "abc123"});
        assert_eq!(
            SlackChannel::url_verification(&payload),
            Some("abc123".to_string())
        );
        // Non-verification payloads return None.
        assert_eq!(
            SlackChannel::url_verification(&json!({"type": "event_callback"})),
            None
        );
    }

    #[test]
    fn parse_inbound_extracts_user_message() {
        let payload = json!({
            "type": "event_callback",
            "event": {
                "type": "message",
                "user": "U123",
                "text": "hello soul",
                "channel": "C456",
                "ts": "1700000000.000100"
            }
        });
        let msgs = SlackChannel::parse_inbound(&payload);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].user, "U123");
        assert_eq!(msgs[0].text, "hello soul");
        assert_eq!(msgs[0].channel, "C456");
        assert_eq!(msgs[0].ts, "1700000000.000100");
    }

    #[test]
    fn parse_inbound_skips_bot_and_subtypes() {
        let bot = json!({
            "type": "event_callback",
            "event": {"type": "message", "bot_id": "B1", "text": "echo", "user": "U1", "channel": "C1", "ts": "1"}
        });
        assert!(SlackChannel::parse_inbound(&bot).is_empty());

        let edit = json!({
            "type": "event_callback",
            "event": {"type": "message", "subtype": "message_changed", "user": "U1", "text": "x", "channel": "C1", "ts": "1"}
        });
        assert!(SlackChannel::parse_inbound(&edit).is_empty());

        // Non-event_callback (e.g. a url_verification) yields nothing.
        assert!(SlackChannel::parse_inbound(&json!({"type": "url_verification"})).is_empty());
    }

    #[test]
    fn verify_signature_lenient_without_secret() {
        let ch = SlackChannel::new();
        assert!(ch.verify_signature("123", b"body", "v0=anything"));
    }

    #[test]
    fn verify_signature_validates_slack_scheme() {
        let mut ch = SlackChannel::new();
        ch.signing_secret = Some("8f742231b10e8888abcd99yyyzzz85a5".into());
        let timestamp = "1531420618";
        let body = b"token=xyz&team_id=T1";
        let basestring = format!("v0:{timestamp}:{}", std::str::from_utf8(body).unwrap());
        let sig = format!(
            "v0={}",
            hmac_sha256_hex("8f742231b10e8888abcd99yyyzzz85a5", &basestring)
        );
        assert!(ch.verify_signature(timestamp, body, &sig));
        // Wrong signature and tampered body both fail.
        assert!(!ch.verify_signature(timestamp, body, "v0=deadbeef"));
        assert!(!ch.verify_signature(timestamp, b"tampered", &sig));
    }

    #[test]
    fn disabled_channel_refuses_send() {
        let ch = SlackChannel::new();
        assert!(!ch.enabled);
    }
}
