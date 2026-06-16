//! # Telegram Provider
//!
//! Long-poll Telegram bot using `teloxide`. Replies to direct messages by
//! forwarding the text to the entity's `ask` handler and sending the response
//! back to the chat.

use super::{emit_event, ChannelConfig, ChannelProvider};
use crate::EntityEvent;
use std::sync::Arc;
use teloxide::prelude::*;

pub struct TelegramProvider {
    token: Option<String>,
}

impl TelegramProvider {
    pub fn from_env() -> Self {
        Self {
            token: std::env::var("TELEGRAM_BOT_TOKEN").ok(),
        }
    }
}

#[async_trait::async_trait]
impl ChannelProvider for TelegramProvider {
    fn name(&self) -> &'static str {
        "telegram"
    }

    async fn start(
        &self,
        cfg: ChannelConfig,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let token = self.token.clone().ok_or("TELEGRAM_BOT_TOKEN not set")?;
        let _entity = cfg.entity.clone();
        let _bus = cfg.event_bus.clone();

        tokio::spawn(async move {
            let bot = Bot::new(token);
            let bus = cfg.event_bus.clone();
            let entity = cfg.entity.clone();

            let handler = Update::filter_message().endpoint(
                move |bot: Bot, msg: Message, entity: Arc<dyn crate::EntityHandle>| {
                    let bus = bus.clone();
                    async move {
                        let chat_id = msg.chat.id;
                        let text = msg.text().unwrap_or("").to_string();

                        emit_event(
                            &bus,
                            EntityEvent::Observation {
                                text: format!("telegram:{chat_id}: {text}"),
                                ts: chrono::Utc::now(),
                            },
                        );

                        let reply = if text.starts_with('/') {
                            handle_command(&entity, &text).await
                        } else {
                            entity
                                .ask(&text)
                                .await
                                .unwrap_or_else(|e| format!("Error: {e}"))
                        };

                        let _ = bot.send_message(chat_id, reply).await;
                        respond(())
                    }
                },
            );

            // Teloxide dispatcher with dependency injection of the entity handle.
            Dispatcher::builder(bot, handler)
                .dependencies(dptree::deps![entity])
                .enable_ctrlc_handler()
                .build()
                .dispatch()
                .await;
        });

        Ok(())
    }

    async fn send_message(
        &self,
        _chat_id: &str,
        _text: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Active outbound sending requires a stored Bot instance. For now the
        // provider is inbound-only; replies are sent during the update handler.
        Ok(())
    }
}

async fn handle_command(entity: &Arc<dyn crate::EntityHandle>, text: &str) -> String {
    let parts: Vec<&str> = text.splitn(2, ' ').collect();
    match parts.first().copied() {
        Some("/start") => "SoulSystem Telegram gateway is active.".to_string(),
        Some("/status") => entity.status().await.to_string(),
        Some("/goal") => {
            if parts.len() < 2 {
                return "Usage: /goal <description>".to_string();
            }
            match entity.create_goal(parts[1]).await {
                Ok(id) => format!("Goal created: {id}"),
                Err(e) => format!("Error: {e}"),
            }
        }
        _ => entity
            .ask(text)
            .await
            .unwrap_or_else(|e| format!("Error: {e}")),
    }
}
