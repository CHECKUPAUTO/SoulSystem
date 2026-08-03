use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, error, info, warn};

/// Telegram provider configuration
#[derive(Debug, Clone)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub api_url: String,
    pub webhook_url: Option<String>,
    pub polling_interval_secs: u64,
    pub require_mention: bool,
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            bot_token: String::new(),
            api_url: "https://api.telegram.org".into(),
            webhook_url: None,
            polling_interval_secs: 5,
            require_mention: false,
        }
    }
}

impl TelegramConfig {
    pub fn from_env() -> Option<Self> {
        let bot_token = std::env::var("TELEGRAM_BOT_TOKEN").ok()?;
        Some(Self {
            bot_token,
            api_url: std::env::var("TELEGRAM_API_URL")
                .unwrap_or_else(|_| "https://api.telegram.org".into()),
            webhook_url: std::env::var("TELEGRAM_WEBHOOK_URL").ok(),
            polling_interval_secs: std::env::var("TELEGRAM_POLLING_INTERVAL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
            require_mention: matches!(
                std::env::var("TELEGRAM_REQUIRE_MENTION").ok().as_deref(),
                Some("1" | "true" | "yes")
            ),
        })
    }
}

/// Telegram update from API
#[derive(Debug, Clone, Deserialize)]
pub struct TelegramUpdate {
    pub update_id: i64,
    pub message: Option<TelegramMessage>,
    pub edited_message: Option<TelegramMessage>,
    pub callback_query: Option<TelegramCallbackQuery>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramMessage {
    pub message_id: i64,
    pub from: Option<TelegramUser>,
    pub date: i64,
    pub chat: TelegramChat,
    pub text: Option<String>,
    pub entities: Option<Vec<TelegramMessageEntity>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramUser {
    pub id: i64,
    pub is_bot: bool,
    pub first_name: String,
    pub last_name: Option<String>,
    pub username: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramChat {
    pub id: i64,
    pub type_: String,
    pub title: Option<String>,
    pub username: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramMessageEntity {
    pub type_: String,
    pub offset: i64,
    pub length: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramCallbackQuery {
    pub id: String,
    pub from: TelegramUser,
    pub message: Option<TelegramMessage>,
    pub data: Option<String>,
}

/// Message to send via Telegram
#[derive(Debug, Clone, Serialize)]
pub struct TelegramSendMessage {
    pub chat_id: i64,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_message_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_markup: Option<Value>,
}

/// Telegram provider handle
pub struct TelegramProvider {
    config: TelegramConfig,
    client: reqwest::Client,
    last_update_id: Option<i64>,
}

impl TelegramProvider {
    pub fn new(config: TelegramConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            last_update_id: None,
        }
    }

    fn api_url(&self, method: &str) -> String {
        format!("{}/bot{}/{}", self.config.api_url, self.config.bot_token, method)
    }

    pub async fn get_updates(&mut self) -> anyhow::Result<Vec<TelegramUpdate>> {
        let mut url = format!("{}?limit=100", self.api_url("getUpdates"));
        if let Some(id) = self.last_update_id {
            url.push_str(&format!("&offset={}", id + 1));
        }

        let response = self.client.get(&url).send().await?;
        let result: TelegramApiResponse<Vec<TelegramUpdate>> = response.json().await?;

        if result.ok {
            if let Some(updates) = result.result {
                if let Some(last) = updates.last() {
                    self.last_update_id = Some(last.update_id);
                }
                Ok(updates)
            } else {
                Ok(vec![])
            }
        } else {
            Err(anyhow::anyhow!(
                "Telegram API error: {:?}",
                result.description
            ))
        }
    }

    pub async fn send_message(&self, msg: TelegramSendMessage) -> anyhow::Result<()> {
        let response = self
            .client
            .post(&self.api_url("sendMessage"))
            .json(&msg)
            .send()
            .await?;

        let result: TelegramApiResponse<TelegramMessage> = response.json().await?;

        if result.ok {
            debug!("Sent message to chat {}", msg.chat_id);
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Failed to send message: {:?}",
                result.description
            ))
        }
    }

    pub async fn set_webhook(&self, url: &str) -> anyhow::Result<()> {
        let response = self
            .client
            .post(&self.api_url("setWebhook"))
            .json(&json!({"url": url}))
            .send()
            .await?;

        let result: TelegramApiResponse<bool> = response.json().await?;

        if result.ok {
            info!("Telegram webhook set to {}", url);
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Failed to set webhook: {:?}",
                result.description
            ))
        }
    }

    pub async fn delete_webhook(&self) -> anyhow::Result<()> {
        let response = self
            .client
            .post(&self.api_url("deleteWebhook"))
            .send()
            .await?;

        let result: TelegramApiResponse<bool> = response.json().await?;

        if result.ok {
            info!("Telegram webhook deleted");
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Failed to delete webhook: {:?}",
                result.description
            ))
        }
    }

    /// Check if message requires mention to be processed
    pub fn requires_mention(&self) -> bool {
        self.config.require_mention
    }

    /// Check if message contains bot mention
    pub fn contains_mention(&self, message: &TelegramMessage, bot_username: &str) -> bool {
        if let Some(entities) = &message.entities {
            for entity in entities {
                if entity.type_ == "mention" {
                    if let Some(text) = &message.text {
                        let mention = &text[entity.offset as usize
                            ..(entity.offset + entity.length) as usize];
                        if mention.eq_ignore_ascii_case(&format!("@{}", bot_username)) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

#[derive(Debug, Clone, Deserialize)]
struct TelegramApiResponse<T> {
    ok: bool,
    result: Option<T>,
    #[serde(rename = "error_code")]
    error_code: Option<i32>,
    description: Option<String>,
}

use serde_json::json;
