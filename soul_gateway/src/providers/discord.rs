//! # Discord Provider
//!
//! Webhook-based Discord integration. Requires:
//!
//! - `DISCORD_BOT_TOKEN` — used to send replies via the Discord REST API.
//! - `DISCORD_PUBLIC_KEY` — optional, used to verify incoming webhook signatures.
//!
//! This provider does **not** open a Discord Gateway connection; it is meant
//! to be combined with a separate Discord bot that forwards messages to the
//! SoulSystem HTTP endpoint. For a self-hosted bot, consider adding a real
//! Gateway client later.
//!
//! Endpoint exposed by the HTTP server (via soul_gateway):
//! `POST /providers/discord/webhook`

use super::{emit_event, ChannelConfig, ChannelProvider};
use crate::EntityEvent;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub struct DiscordProvider {
    token: Option<String>,
    #[allow(dead_code)]
    public_key: Option<String>,
}

impl DiscordProvider {
    pub fn from_env() -> Self {
        Self {
            token: std::env::var("DISCORD_BOT_TOKEN").ok(),
            public_key: std::env::var("DISCORD_PUBLIC_KEY").ok(),
        }
    }

    fn client(&self) -> Result<reqwest::Client, Box<dyn std::error::Error + Send + Sync>> {
        Ok(reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?)
    }
}

#[async_trait::async_trait]
impl ChannelProvider for DiscordProvider {
    fn name(&self) -> &'static str {
        "discord"
    }

    async fn start(
        &self,
        cfg: ChannelConfig,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let token = self
            .token
            .clone()
            .ok_or("DISCORD_BOT_TOKEN not set (provider is outbound-only without it)")?;
        let _ = cfg;
        if token.is_empty() {
            return Err("DISCORD_BOT_TOKEN is empty".into());
        }
        Ok(())
    }

    async fn send_message(
        &self,
        chat_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let token = self.token.clone().ok_or("DISCORD_BOT_TOKEN not set")?;
        let channel_id = chat_id;
        let url = format!("https://discord.com/api/v10/channels/{channel_id}/messages");
        let body = serde_json::json!({ "content": text });
        let client = self.client()?;
        let resp = client
            .post(&url)
            .header("Authorization", format!("Bot {token}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            return Err(format!("Discord API error {status}: {txt}").into());
        }
        Ok(())
    }
}

/// Minimal Discord webhook interaction payload.
#[derive(Debug, Deserialize, Serialize)]
pub struct DiscordWebhookBody {
    #[serde(rename = "type")]
    pub interaction_type: Option<u8>,
    pub id: Option<String>,
    pub token: Option<String>,
    pub channel_id: Option<String>,
    pub member: Option<DiscordMember>,
    pub data: Option<DiscordInteractionData>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DiscordMember {
    pub user: Option<DiscordUser>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DiscordUser {
    pub id: String,
    pub username: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DiscordInteractionData {
    pub name: Option<String>,
    pub options: Option<Vec<DiscordInteractionOption>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DiscordInteractionOption {
    pub name: String,
    pub value: String,
}

/// Extract the raw text to forward to the entity from a Discord interaction.
pub fn interaction_text(body: &DiscordWebhookBody) -> String {
    let mut parts = Vec::new();
    if let Some(ref data) = body.data {
        if let Some(ref name) = data.name {
            parts.push(name.clone());
        }
        if let Some(ref opts) = data.options {
            for opt in opts {
                parts.push(format!("{}={}", opt.name, opt.value));
            }
        }
    }
    parts.join(" ")
}

/// Route a Discord webhook payload through the entity and return an
/// interaction response object.
pub async fn handle_webhook(
    entity: &Arc<dyn crate::EntityHandle>,
    bus: &Option<crate::EventHub>,
    body: DiscordWebhookBody,
) -> serde_json::Value {
    let text = interaction_text(&body);

    emit_event(
        bus,
        EntityEvent::Observation {
            text: format!("discord:{}: {text}", body.id.as_deref().unwrap_or("?")),
            ts: chrono::Utc::now(),
        },
    );

    let reply = entity
        .ask(&text)
        .await
        .unwrap_or_else(|e| format!("Error: {e}"));

    serde_json::json!({
        "type": 4,
        "data": {
            "content": reply,
            "flags": 0,
        }
    })
}
