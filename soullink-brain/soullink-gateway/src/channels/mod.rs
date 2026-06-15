//! Messaging channel implementations.
//!
//! Each channel implements a common trait for message send/receive.
//! Currently:
//! - Telegram (full client in `src/telegram/`)
//! - Signal (via signal-cli REST API)
//! - WhatsApp (WhatsApp Business Cloud API — send, inbound webhook parsing,
//!   subscription handshake, and HMAC signature verification)
//! - Slack (Web API send + Events API inbound, URL-verification handshake, and
//!   request-signature verification)
//! - Discord (REST send + Interactions webhook, Ed25519 signature verification,
//!   PING/PONG handshake, command parsing)

pub mod discord;
pub mod signal;
pub mod slack;
pub mod whatsapp;

use async_trait::async_trait;

/// A uniform outbound interface across messaging channels, so the gateway can
/// hold a heterogeneous set of channels and dispatch a reply without caring
/// which backend it targets. `target` is the channel's address (phone number,
/// Slack/Discord channel ID, …) and `text` is the message body.
#[async_trait]
pub trait Channel: Send + Sync {
    /// Stable channel identifier (e.g. `"whatsapp"`, `"slack"`).
    fn name(&self) -> &str;
    /// Whether the channel has credentials and can send.
    fn enabled(&self) -> bool;
    /// Send a text message to `target`.
    async fn send(&self, target: &str, text: &str) -> Result<(), String>;
}

#[async_trait]
impl Channel for whatsapp::WhatsAppChannel {
    fn name(&self) -> &str {
        "whatsapp"
    }
    fn enabled(&self) -> bool {
        self.enabled
    }
    async fn send(&self, target: &str, text: &str) -> Result<(), String> {
        self.send_message(target, text).await
    }
}

#[async_trait]
impl Channel for slack::SlackChannel {
    fn name(&self) -> &str {
        "slack"
    }
    fn enabled(&self) -> bool {
        self.enabled
    }
    async fn send(&self, target: &str, text: &str) -> Result<(), String> {
        self.send_message(target, text).await
    }
}

#[async_trait]
impl Channel for discord::DiscordChannel {
    fn name(&self) -> &str {
        "discord"
    }
    fn enabled(&self) -> bool {
        self.enabled
    }
    async fn send(&self, target: &str, text: &str) -> Result<(), String> {
        self.send_message(target, text).await
    }
}

#[async_trait]
impl Channel for signal::SignalChannel {
    fn name(&self) -> &str {
        "signal"
    }
    fn enabled(&self) -> bool {
        self.enabled
    }
    async fn send(&self, target: &str, text: &str) -> Result<(), String> {
        self.send_message(target, text).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn channels_share_a_dispatchable_interface() {
        let channels: Vec<Box<dyn Channel>> = vec![
            Box::new(whatsapp::WhatsAppChannel::new()),
            Box::new(slack::SlackChannel::new()),
            Box::new(discord::DiscordChannel::new()),
        ];
        let names: Vec<&str> = channels.iter().map(|c| c.name()).collect();
        assert_eq!(names, ["whatsapp", "slack", "discord"]);

        // All are disabled (no creds) and refuse to send through the trait.
        for c in &channels {
            assert!(!c.enabled());
            assert!(c.send("target", "hi").await.is_err());
        }
    }
}
