//! WhatsApp channel — WhatsApp Business Cloud API integration.
//!
//! Outbound messages are sent via the Graph API
//! (`POST https://graph.facebook.com/{version}/{phone_number_id}/messages`)
//! with a bearer access token. Inbound messages arrive as webhooks; this
//! module parses the `entry[].changes[].value.messages[]` payload, verifies
//! the `X-Hub-Signature-256` HMAC, and answers the GET subscription challenge.
//!
//! Reference: <https://developers.facebook.com/docs/whatsapp/cloud-api>

use hmac::{Hmac, Mac};
use sha2::Sha256;
use tracing::{info, warn};

const DEFAULT_GRAPH_BASE: &str = "https://graph.facebook.com";
const DEFAULT_GRAPH_VERSION: &str = "v21.0";

type HmacSha256 = Hmac<Sha256>;

/// A single inbound text message extracted from a WhatsApp webhook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundMessage {
    /// Sender's phone number (E.164, no `+`), as WhatsApp reports it.
    pub from: String,
    /// Message text body.
    pub text: String,
    /// WhatsApp message ID (`wamid....`).
    pub message_id: String,
}

pub struct WhatsAppChannel {
    pub enabled: bool,
    /// Graph API base, e.g. `https://graph.facebook.com`.
    pub api_base: String,
    /// Graph API version, e.g. `v21.0`.
    pub api_version: String,
    /// Phone number ID the messages are sent from.
    pub phone_number_id: Option<String>,
    /// Bearer access token (system-user or temporary token).
    /// Held in `SecretString` so the credential cannot reach a log line or a
    /// `{:?}` render of the enclosing config (INV-SEC-2).
    pub access_token: Option<soulsystem_common::secrets::SecretString>,
    /// Token echoed back during webhook subscription (GET handshake).
    pub verify_token: Option<String>,
    /// App secret used to verify the `X-Hub-Signature-256` HMAC.
    pub app_secret: Option<String>,
}

impl Default for WhatsAppChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl WhatsAppChannel {
    /// A disabled channel with Graph API defaults — fill in credentials to
    /// enable.
    pub fn new() -> Self {
        Self {
            enabled: false,
            api_base: DEFAULT_GRAPH_BASE.to_string(),
            api_version: DEFAULT_GRAPH_VERSION.to_string(),
            phone_number_id: None,
            access_token: None,
            verify_token: None,
            app_secret: None,
        }
    }

    /// Build an enabled channel from the required credentials.
    pub fn configured(phone_number_id: impl Into<String>, access_token: impl Into<String>) -> Self {
        Self {
            enabled: true,
            phone_number_id: Some(phone_number_id.into()),
            access_token: Some(soulsystem_common::secrets::SecretString::new(access_token)),
            ..Self::new()
        }
    }

    /// `{api_base}/{api_version}/{phone_number_id}/messages`.
    fn messages_url(&self) -> Option<String> {
        let pnid = self.phone_number_id.as_ref()?;
        Some(format!(
            "{}/{}/{}/messages",
            self.api_base.trim_end_matches('/'),
            self.api_version,
            pnid
        ))
    }

    /// Send a text message via the WhatsApp Cloud API.
    pub async fn send_message(&self, recipient: &str, text: &str) -> Result<(), String> {
        if !self.enabled {
            return Err("WhatsApp channel is disabled (no credentials configured)".into());
        }
        let url = self
            .messages_url()
            .ok_or("WhatsApp phone_number_id not configured")?;
        let token = self
            .access_token
            .as_ref()
            .ok_or("WhatsApp access_token not configured")?;

        let client = soullink_core::http::shared_client();
        let resp = client
            .post(&url)
            .bearer_auth(token.expose())
            .json(&serde_json::json!({
                "messaging_product": "whatsapp",
                "recipient_type": "individual",
                "to": recipient,
                "type": "text",
                "text": { "preview_url": false, "body": text },
            }))
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| format!("whatsapp send failed: {e}"))?;

        let status = resp.status();
        if status.is_success() {
            info!(recipient = %recipient, "WhatsApp message sent");
            Ok(())
        } else {
            let body = resp.text().await.unwrap_or_default();
            warn!(status = status.as_u16(), body = %body, "WhatsApp send error");
            Err(format!("HTTP {status}: {body}"))
        }
    }

    /// Answer the webhook subscription GET handshake. Returns the challenge to
    /// echo when `mode == "subscribe"` and the token matches `verify_token`.
    pub fn verify_subscription(&self, mode: &str, token: &str, challenge: &str) -> Option<String> {
        match self.verify_token.as_deref() {
            Some(expected) if mode == "subscribe" && token == expected => {
                Some(challenge.to_string())
            }
            _ => None,
        }
    }

    /// Verify the `X-Hub-Signature-256` header against the raw request body
    /// using the configured app secret. The header is `sha256=<hex>`.
    ///
    /// Returns `true` if no `app_secret` is configured (verification disabled),
    /// matching the lenient posture of the existing webhook ingress; callers
    /// that require verification should ensure `app_secret` is set.
    pub fn verify_signature(&self, raw_body: &[u8], signature_header: &str) -> bool {
        let Some(secret) = self.app_secret.as_ref() else {
            return true;
        };
        let Some(hex) = signature_header.strip_prefix("sha256=") else {
            return false;
        };
        let Some(provided) = decode_hex(hex) else {
            return false;
        };
        let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
            Ok(m) => m,
            Err(_) => return false,
        };
        mac.update(raw_body);
        // `verify_slice` is constant-time.
        mac.verify_slice(&provided).is_ok()
    }

    /// Parse the inbound text messages out of a WhatsApp webhook payload.
    /// Non-text messages (media, statuses, reactions) are skipped.
    pub fn parse_inbound(payload: &serde_json::Value) -> Vec<InboundMessage> {
        let mut out = Vec::new();
        let Some(entries) = payload["entry"].as_array() else {
            return out;
        };
        for entry in entries {
            let Some(changes) = entry["changes"].as_array() else {
                continue;
            };
            for change in changes {
                let value = &change["value"];
                let Some(messages) = value["messages"].as_array() else {
                    continue;
                };
                for msg in messages {
                    if msg["type"] != "text" {
                        continue;
                    }
                    let (Some(from), Some(text), Some(id)) = (
                        msg["from"].as_str(),
                        msg["text"]["body"].as_str(),
                        msg["id"].as_str(),
                    ) else {
                        continue;
                    };
                    out.push(InboundMessage {
                        from: from.to_string(),
                        text: text.to_string(),
                        message_id: id.to_string(),
                    });
                }
            }
        }
        out
    }
}

/// Decode a lowercase/uppercase hex string into bytes. Returns `None` on any
/// non-hex character or odd length.
fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    let nibble = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        }
    };
    let (pairs, _) = bytes.as_chunks::<2>();
    for pair in pairs {
        let hi = nibble(pair[0])?;
        let lo = nibble(pair[1])?;
        out.push((hi << 4) | lo);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmac::Mac;
    use serde_json::json;

    #[test]
    fn messages_url_uses_version_and_phone_id() {
        let ch = WhatsAppChannel::configured("123456", "tok");
        assert_eq!(
            ch.messages_url().unwrap(),
            "https://graph.facebook.com/v21.0/123456/messages"
        );
    }

    #[test]
    fn disabled_channel_refuses_send() {
        // No tokio runtime needed: send_message short-circuits before any await
        // on the disabled path, but to keep it simple we just assert the guard.
        let ch = WhatsAppChannel::new();
        assert!(!ch.enabled);
        assert!(ch.messages_url().is_none());
    }

    #[test]
    fn verify_subscription_echoes_challenge_on_match() {
        let mut ch = WhatsAppChannel::new();
        ch.verify_token = Some("s3cr3t".into());
        assert_eq!(
            ch.verify_subscription("subscribe", "s3cr3t", "CHALLENGE"),
            Some("CHALLENGE".to_string())
        );
        // Wrong token, wrong mode, or unset token → None.
        assert_eq!(
            ch.verify_subscription("subscribe", "nope", "CHALLENGE"),
            None
        );
        assert_eq!(
            ch.verify_subscription("unsubscribe", "s3cr3t", "CHALLENGE"),
            None
        );
        assert_eq!(
            WhatsAppChannel::new().verify_subscription("subscribe", "x", "C"),
            None
        );
    }

    #[test]
    fn verify_signature_accepts_valid_hmac() {
        let mut ch = WhatsAppChannel::new();
        ch.app_secret = Some("app-secret".into());
        let body = br#"{"hello":"world"}"#;

        let mut mac = HmacSha256::new_from_slice(b"app-secret").unwrap();
        mac.update(body);
        let sig = mac.finalize().into_bytes();
        let header = format!("sha256={}", encode_hex(&sig));

        assert!(ch.verify_signature(body, &header));
        // Tampered body fails.
        assert!(!ch.verify_signature(b"{}", &header));
        // Malformed header fails.
        assert!(!ch.verify_signature(body, "garbage"));
    }

    #[test]
    fn verify_signature_lenient_without_secret() {
        let ch = WhatsAppChannel::new();
        assert!(ch.verify_signature(b"anything", "sha256=deadbeef"));
    }

    #[test]
    fn parse_inbound_extracts_text_messages() {
        let payload = json!({
            "entry": [{
                "changes": [{
                    "value": {
                        "messages": [
                            {
                                "from": "15551234567",
                                "id": "wamid.ABC",
                                "type": "text",
                                "text": { "body": "hello soul" }
                            },
                            {
                                "from": "15551234567",
                                "id": "wamid.IMG",
                                "type": "image",
                                "image": { "id": "media123" }
                            }
                        ]
                    }
                }]
            }]
        });
        let msgs = WhatsAppChannel::parse_inbound(&payload);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].from, "15551234567");
        assert_eq!(msgs[0].text, "hello soul");
        assert_eq!(msgs[0].message_id, "wamid.ABC");
    }

    #[test]
    fn parse_inbound_handles_status_only_payload() {
        // Delivery/read receipts carry `statuses`, not `messages`.
        let payload = json!({
            "entry": [{ "changes": [{ "value": { "statuses": [{ "status": "delivered" }] } }] }]
        });
        assert!(WhatsAppChannel::parse_inbound(&payload).is_empty());
    }

    #[test]
    fn decode_hex_roundtrip() {
        assert_eq!(decode_hex("00ff10"), Some(vec![0x00, 0xff, 0x10]));
        assert_eq!(decode_hex("abc"), None); // odd length
        assert_eq!(decode_hex("zz"), None); // non-hex
    }

    fn encode_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}
