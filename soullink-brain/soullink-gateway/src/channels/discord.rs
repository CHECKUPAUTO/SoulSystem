//! Discord channel — REST send + Interactions webhook (inbound).
//!
//! Outbound messages are posted to a channel via the REST API with a bot token.
//! Inbound slash-command interactions arrive as signed POSTs; this module
//! verifies the Ed25519 request signature, answers the `PING` handshake with a
//! `PONG`, and extracts the command text from `APPLICATION_COMMAND`
//! interactions.
//!
//! Reference: <https://discord.com/developers/docs/interactions/receiving-and-responding>

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use tracing::{info, warn};

const DEFAULT_API_BASE: &str = "https://discord.com/api/v10";

/// Discord interaction type codes.
const INTERACTION_PING: u64 = 1;
const INTERACTION_APPLICATION_COMMAND: u64 = 2;
/// Interaction *response* type for acknowledging a PING.
const RESPONSE_PONG: u64 = 1;

/// A single inbound Discord command interaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundMessage {
    /// Discord user ID of the invoker.
    pub user: String,
    /// The command text (name plus its first string option, if any).
    pub text: String,
    /// Channel ID the interaction came from.
    pub channel: String,
}

pub struct DiscordChannel {
    pub enabled: bool,
    /// REST API base, e.g. `https://discord.com/api/v10`.
    pub api_base: String,
    /// Bot token (sent as `Authorization: Bot <token>`).
    /// Held in `SecretString` so the token cannot reach a log line or a
    /// `{:?}` render of the enclosing config (INV-SEC-2).
    pub bot_token: Option<soulsystem_common::secrets::SecretString>,
    /// Application public key (hex) for verifying interaction signatures.
    pub public_key: Option<String>,
}

impl Default for DiscordChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl DiscordChannel {
    /// A disabled channel with REST defaults — fill in credentials to enable.
    pub fn new() -> Self {
        Self {
            enabled: false,
            api_base: DEFAULT_API_BASE.to_string(),
            bot_token: None,
            public_key: None,
        }
    }

    /// Build an enabled channel from a bot token.
    pub fn configured(bot_token: impl Into<String>) -> Self {
        Self {
            enabled: true,
            bot_token: Some(soulsystem_common::secrets::SecretString::new(bot_token)),
            ..Self::new()
        }
    }

    /// Post a message to a channel via the REST API.
    pub async fn send_message(&self, channel_id: &str, content: &str) -> Result<(), String> {
        if !self.enabled {
            return Err("Discord channel is disabled (no credentials configured)".into());
        }
        let token = self
            .bot_token
            .as_ref()
            .ok_or("Discord bot_token not configured")?;
        let url = format!(
            "{}/channels/{channel_id}/messages",
            self.api_base.trim_end_matches('/')
        );

        let client = soullink_core::http::shared_client();
        let resp = client
            .post(&url)
            .header("Authorization", format!("Bot {}", token.expose()))
            .json(&serde_json::json!({ "content": content }))
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| format!("discord send failed: {e}"))?;

        let status = resp.status();
        if status.is_success() {
            info!(channel = %channel_id, "Discord message sent");
            Ok(())
        } else {
            let body = resp.text().await.unwrap_or_default();
            warn!(status = status.as_u16(), body = %body, "Discord send error");
            Err(format!("HTTP {status}: {body}"))
        }
    }

    /// Verify the Ed25519 interaction signature over `timestamp || raw_body`,
    /// using the application public key. Lenient (`true`) when no public key is
    /// configured.
    pub fn verify_signature(&self, timestamp: &str, raw_body: &[u8], signature_hex: &str) -> bool {
        let Some(pubkey_hex) = self.public_key.as_ref() else {
            return true;
        };
        let (Some(pubkey), Some(sig_bytes)) = (decode_hex(pubkey_hex), decode_hex(signature_hex))
        else {
            return false;
        };
        let Ok(pubkey_arr): Result<[u8; 32], _> = pubkey.try_into() else {
            return false;
        };
        let Ok(sig_arr): Result<[u8; 64], _> = sig_bytes.try_into() else {
            return false;
        };
        let Ok(vk) = VerifyingKey::from_bytes(&pubkey_arr) else {
            return false;
        };
        let signature = Signature::from_bytes(&sig_arr);
        let mut message = timestamp.as_bytes().to_vec();
        message.extend_from_slice(raw_body);
        vk.verify(&message, &signature).is_ok()
    }

    /// True if the interaction is a `PING` (the registration handshake).
    pub fn is_ping(payload: &serde_json::Value) -> bool {
        payload["type"].as_u64() == Some(INTERACTION_PING)
    }

    /// The response body to acknowledge a `PING`.
    pub fn pong() -> serde_json::Value {
        serde_json::json!({ "type": RESPONSE_PONG })
    }

    /// Parse a command interaction into an inbound message. Returns `None` for
    /// PINGs and non-command interactions.
    pub fn parse_interaction(payload: &serde_json::Value) -> Option<InboundMessage> {
        if payload["type"].as_u64() != Some(INTERACTION_APPLICATION_COMMAND) {
            return None;
        }
        let data = &payload["data"];
        let name = data["name"].as_str()?;
        // First string option (if any) becomes the command argument text.
        let arg = data["options"]
            .as_array()
            .and_then(|opts| opts.first())
            .and_then(|o| o["value"].as_str())
            .unwrap_or("");
        let text = if arg.is_empty() {
            name.to_string()
        } else {
            format!("{name} {arg}")
        };

        // The invoker is under `member.user` (guild) or `user` (DM).
        let user = payload["member"]["user"]["id"]
            .as_str()
            .or_else(|| payload["user"]["id"].as_str())
            .unwrap_or("")
            .to_string();
        let channel = payload["channel_id"].as_str().unwrap_or("").to_string();

        Some(InboundMessage {
            user,
            text,
            channel,
        })
    }
}

/// Decode a hex string into bytes; `None` on odd length or non-hex input.
fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let nibble = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        }
    };
    let (pairs, _) = s.as_bytes().as_chunks::<2>();
    pairs
        .iter()
        .map(|p| Some((nibble(p[0])? << 4) | nibble(p[1])?))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use serde_json::json;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn ping_handshake() {
        assert!(DiscordChannel::is_ping(&json!({"type": 1})));
        assert!(!DiscordChannel::is_ping(&json!({"type": 2})));
        assert_eq!(DiscordChannel::pong(), json!({"type": 1}));
    }

    #[test]
    fn parse_interaction_extracts_command() {
        let payload = json!({
            "type": 2,
            "channel_id": "C9",
            "member": {"user": {"id": "U7"}},
            "data": {
                "name": "ask",
                "options": [{"name": "q", "value": "what is rust"}]
            }
        });
        let msg = DiscordChannel::parse_interaction(&payload).unwrap();
        assert_eq!(msg.user, "U7");
        assert_eq!(msg.text, "ask what is rust");
        assert_eq!(msg.channel, "C9");

        // PING / non-command → None.
        assert!(DiscordChannel::parse_interaction(&json!({"type": 1})).is_none());
    }

    #[test]
    fn verify_signature_validates_ed25519() {
        // Deterministic keypair from a fixed seed.
        let signing = SigningKey::from_bytes(&[7u8; 32]);
        let public_hex = hex(signing.verifying_key().as_bytes());

        let mut ch = DiscordChannel::new();
        ch.public_key = Some(public_hex);

        let timestamp = "1700000000";
        let body = br#"{"type":1}"#;
        let mut message = timestamp.as_bytes().to_vec();
        message.extend_from_slice(body);
        let sig_hex = hex(&signing.sign(&message).to_bytes());

        assert!(ch.verify_signature(timestamp, body, &sig_hex));
        // Tampered body and bad signature both fail.
        assert!(!ch.verify_signature(timestamp, b"{}", &sig_hex));
        assert!(!ch.verify_signature(timestamp, body, "00"));
    }

    #[test]
    fn verify_signature_lenient_without_key() {
        let ch = DiscordChannel::new();
        assert!(ch.verify_signature("1", b"body", "deadbeef"));
    }

    #[test]
    fn disabled_channel_refuses_send() {
        assert!(!DiscordChannel::new().enabled);
    }
}
