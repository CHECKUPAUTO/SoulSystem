//! # Channel Providers — Messaging platform integrations for SoulSystem
//!
//! Each provider implements the `ChannelProvider` trait and bridges a
//! messaging surface (Telegram, Discord, WhatsApp, ...) to the autonomous
//! entity. Providers are optional: they start only when their required
//! environment variables / tokens are present.

use crate::{EntityEvent, EntityHandle};
use std::sync::Arc;

pub mod discord;
pub mod telegram;

/// Configuration common to all channel providers.
#[derive(Clone)]
pub struct ChannelConfig {
    pub entity: Arc<dyn EntityHandle>,
    pub event_bus: Option<crate::EventHub>,
}

/// A channel provider can receive messages from a platform and send replies
/// back to users. It is also responsible for its own lifecycle (spawn a
/// background task, set up webhooks, etc.).
#[async_trait::async_trait]
pub trait ChannelProvider: Send + Sync {
    /// Human-readable name of the provider.
    fn name(&self) -> &'static str;

    /// Try to start the provider. Returns an error if configuration is missing
    /// or invalid. On success the provider usually spawns a background task
    /// and returns immediately.
    async fn start(
        &self,
        cfg: ChannelConfig,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Send a text message to a user / chat on this platform.
    async fn send_message(
        &self,
        chat_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// Start every provider whose configuration is present in the environment.
/// Failures are logged but do not stop the other providers.
pub async fn start_all(config: ChannelConfig) {
    let providers: Vec<Box<dyn ChannelProvider>> = vec![
        Box::new(telegram::TelegramProvider::from_env()),
        Box::new(discord::DiscordProvider::from_env()),
        // WhatsAppProvider::from_env(),
    ];

    for p in providers {
        let name = p.name();
        match p.start(config.clone()).await {
            Ok(()) => tracing::info!("{name}: channel provider started"),
            Err(e) => tracing::warn!("{name}: not started: {e}"),
        }
    }
}

/// Helper used by providers to notify the gateway event hub.
pub(crate) fn emit_event(bus: &Option<crate::EventHub>, ev: EntityEvent) {
    if let Some(b) = bus {
        b.publish(ev);
    }
}
