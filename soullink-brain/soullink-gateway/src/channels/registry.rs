//! Channel registry — holds the configured outbound channels and dispatches a
//! reply by provider name. This is the consumer of the [`Channel`] trait: given
//! `(provider, target, text)` it routes to the right backend so the gateway can
//! answer an inbound message on whatever channel it arrived from.

use std::collections::HashMap;

use super::discord::DiscordChannel;
use super::signal::SignalChannel;
use super::slack::SlackChannel;
use super::whatsapp::WhatsAppChannel;
use super::Channel;

/// A set of configured outbound channels keyed by provider name.
#[derive(Default)]
pub struct ChannelRegistry {
    channels: HashMap<String, Box<dyn Channel>>,
}

impl ChannelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or replace) a channel under its own `name()`.
    pub fn register(&mut self, channel: Box<dyn Channel>) {
        self.channels.insert(channel.name().to_string(), channel);
    }

    /// Build the registry from environment credentials, registering only the
    /// channels whose required variables are present.
    pub fn from_env() -> Self {
        let mut reg = Self::new();
        if let (Ok(pnid), Ok(token)) = (
            std::env::var("WHATSAPP_PHONE_NUMBER_ID"),
            std::env::var("WHATSAPP_ACCESS_TOKEN"),
        ) {
            reg.register(Box::new(WhatsAppChannel::configured(pnid, token)));
        }
        if let Ok(token) = std::env::var("SLACK_BOT_TOKEN") {
            reg.register(Box::new(SlackChannel::configured(token)));
        }
        if let Ok(token) = std::env::var("DISCORD_BOT_TOKEN") {
            reg.register(Box::new(DiscordChannel::configured(token)));
        }
        if let (Ok(url), Ok(number)) = (
            std::env::var("SIGNAL_API_URL"),
            std::env::var("SIGNAL_NUMBER"),
        ) {
            reg.register(Box::new(SignalChannel::new(url, number)));
        }
        if let Ok(token) = std::env::var("TELEGRAM_BOT_TOKEN") {
            reg.register(Box::new(crate::telegram::client::TelegramClient::new(
                "https://api.telegram.org",
                token,
            )));
        }
        reg
    }

    /// Names of the registered channels.
    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.channels.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    pub fn len(&self) -> usize {
        self.channels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.channels.is_empty()
    }

    /// Dispatch a message to `target` on the named channel.
    pub async fn dispatch(&self, provider: &str, target: &str, text: &str) -> Result<(), String> {
        match self.channels.get(provider) {
            Some(channel) if channel.enabled() => channel.send(target, text).await,
            Some(_) => Err(format!("channel '{provider}' is disabled")),
            None => Err(format!("no channel configured for '{provider}'")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dispatch_errors_on_unknown_and_disabled() {
        let mut reg = ChannelRegistry::new();
        // A disabled channel (no creds) registered under "whatsapp".
        reg.register(Box::new(WhatsAppChannel::new()));
        assert_eq!(reg.names(), ["whatsapp"]);
        assert_eq!(reg.len(), 1);

        // Unknown provider.
        assert!(reg
            .dispatch("telegram", "123", "hi")
            .await
            .unwrap_err()
            .contains("no channel"));
        // Known but disabled.
        assert!(reg
            .dispatch("whatsapp", "123", "hi")
            .await
            .unwrap_err()
            .contains("disabled"));
    }

    #[test]
    fn from_env_constructs_consistently() {
        // Smoke test: building from env never panics and names/len agree.
        let reg = ChannelRegistry::from_env();
        assert_eq!(reg.names().len(), reg.len());
    }
}
