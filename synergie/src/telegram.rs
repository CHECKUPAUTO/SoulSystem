//! Telegram Bot API minimaliste.

use crate::types::Synergy;
use anyhow::Result;
use serde::Serialize;
use std::time::Duration;
use tracing::{debug, warn};

#[derive(Debug, Clone)]
pub struct Telegram {
    bot: Option<String>,
    chat: Option<String>,
    http: reqwest::Client,
}

#[derive(Debug, Serialize)]
struct SendPayload<'a> {
    chat_id: &'a str,
    text: &'a str,
    parse_mode: &'static str,
    disable_web_page_preview: bool,
}

impl Telegram {
    pub fn new(bot: Option<String>, chat: Option<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { bot, chat, http }
    }

    pub fn is_configured(&self) -> bool {
        self.bot.is_some() && self.chat.is_some()
    }

    pub async fn send(&self, text: &str) -> Result<bool> {
        let (Some(bot), Some(chat)) = (self.bot.as_ref(), self.chat.as_ref()) else {
            debug!(target: "telegram", "non configuré — message ignoré");
            return Ok(false);
        };
        let url = format!("https://api.telegram.org/bot{}/sendMessage", bot);
        let payload = SendPayload {
            chat_id: chat,
            text: &truncate(text, 3900),
            parse_mode: "Markdown",
            disable_web_page_preview: true,
        };
        let resp = self.http.post(&url).json(&payload).send().await?;
        if !resp.status().is_success() {
            let st = resp.status();
            let body = resp.text().await.unwrap_or_default();
            warn!(target: "telegram", status = %st, body = %body, "envoi échoué");
            return Ok(false);
        }
        Ok(true)
    }

    pub async fn send_synergy(&self, s: &Synergy) -> Result<bool> {
        let msg = format!(
            "🧬 *SYNERGIE détectée* (score {:.2})\n*{}* — `{}`\nType : `{}` | Effort : {} | Valeur : {}\n`{}` → `{}`\n\n{}",
            s.adjusted_score,
            s.description.lines().next().unwrap_or(""),
            s.id,
            s.synergy_type,
            s.effort,
            s.value,
            s.source_project,
            s.target_project,
            s.suggested_action.as_deref().unwrap_or("")
        );
        self.send(&msg).await
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { return s.to_string(); }
    let mut end = max;
    while !s.is_char_boundary(end) && end > 0 { end -= 1; }
    format!("{}…", &s[..end])
}
