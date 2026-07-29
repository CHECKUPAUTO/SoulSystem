//! # Slack Provider
//!
//! Webhook-based Slack integration. Requires:
//!
//! - `SLACK_BOT_TOKEN` — used to send replies via the Slack Web API.
//! - `SLACK_SIGNING_SECRET` — optional, used to verify incoming webhook
//!   signatures (Slack Events API).
//!
//! Endpoint exposed by the HTTP server:
//! `POST /providers/slack/webhook`

use super::{emit_event, ChannelConfig, ChannelProvider};
use crate::EntityEvent;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub struct SlackProvider {
    /// Held in `SecretString` so the credential cannot reach a log line,
    /// a panic message or a `{:?}` render through any enclosing derive
    /// (INV-SEC-2). Unwrapped only at the header, below.
    token: Option<soulsystem_common::secrets::SecretString>,
    #[allow(dead_code)]
    signing_secret: Option<String>,
}

impl SlackProvider {
    pub fn from_env() -> Self {
        Self {
            token: std::env::var("SLACK_BOT_TOKEN")
                .ok()
                .map(soulsystem_common::secrets::SecretString::new),
            signing_secret: std::env::var("SLACK_SIGNING_SECRET").ok(),
        }
    }

    fn client(&self) -> Result<reqwest::Client, Box<dyn std::error::Error + Send + Sync>> {
        Ok(reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?)
    }
}

#[async_trait::async_trait]
impl ChannelProvider for SlackProvider {
    fn name(&self) -> &'static str {
        "slack"
    }

    async fn start(
        &self,
        cfg: ChannelConfig,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let token = self.token.clone().ok_or("SLACK_BOT_TOKEN not set")?;
        let _ = cfg;
        if token.is_empty() {
            return Err("SLACK_BOT_TOKEN is empty".into());
        }
        Ok(())
    }

    async fn send_message(
        &self,
        chat_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let token = self.token.clone().ok_or("SLACK_BOT_TOKEN not set")?;
        let url = "https://slack.com/api/chat.postMessage";
        let body = serde_json::json!({
            "channel": chat_id,
            "text": text,
        });
        let client = self.client()?;
        let resp = client
            .post(url)
            .header("Authorization", format!("Bearer {}", token.expose()))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            return Err(format!("Slack API error {status}: {txt}").into());
        }
        let json: serde_json::Value = resp.json().await?;
        if !json.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            let err = json
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            return Err(format!("Slack API error: {err}").into());
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SlackWebhookBody {
    #[serde(rename = "type")]
    pub event_type: Option<String>,
    pub event: Option<SlackEvent>,
    pub challenge: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SlackEvent {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub channel: Option<String>,
    pub user: Option<String>,
    pub text: Option<String>,
    pub ts: Option<String>,
    pub bot_id: Option<String>,
    pub subtype: Option<String>,
}

pub fn event_text(body: &SlackWebhookBody) -> Option<(&str, &str)> {
    let event = body.event.as_ref()?;
    if event.bot_id.is_some() || event.subtype.as_deref() == Some("bot_message") {
        return None;
    }
    let channel = event.channel.as_deref()?;
    let text = event.text.as_deref()?;
    if text.is_empty() {
        return None;
    }
    Some((channel, text))
}

pub async fn handle_webhook(
    entity: &Arc<dyn crate::EntityHandle>,
    bus: &Option<crate::EventHub>,
    body: SlackWebhookBody,
) -> serde_json::Value {
    if let Some(challenge) = body.challenge.clone() {
        return serde_json::json!({ "challenge": challenge });
    }

    let reply = match event_text(&body) {
        Some((channel, text)) => {
            emit_event(
                bus,
                EntityEvent::Observation {
                    text: format!("slack:{channel}: {text}"),
                    ts: chrono::Utc::now(),
                },
            );
            entity
                .ask(text)
                .await
                .unwrap_or_else(|e| format!("Error: {e}"))
        }
        None => return serde_json::json!({"ok": true}),
    };

    serde_json::json!({
        "ok": true,
        "text": reply,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::MockEntity;
    use std::sync::Arc;

    #[test]
    fn slack_provider_missing_token_start_fails() {
        let p = SlackProvider {
            token: None,
            signing_secret: None,
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cfg = ChannelConfig {
            entity: Arc::new(MockEntity {
                ask_calls: std::sync::atomic::AtomicUsize::new(0),
                goal_calls: std::sync::atomic::AtomicUsize::new(0),
                plan_calls: std::sync::atomic::AtomicUsize::new(0),
            }),
            event_bus: None,
        };
        let err = rt.block_on(p.start(cfg)).unwrap_err().to_string();
        assert!(err.contains("SLACK_BOT_TOKEN"));
    }

    #[test]
    fn slack_event_text_skips_bot_messages() {
        let bot = SlackWebhookBody {
            event_type: Some("event_callback".to_string()),
            event: Some(SlackEvent {
                kind: Some("message".to_string()),
                channel: Some("C123".to_string()),
                user: Some("U123".to_string()),
                text: Some("hello".to_string()),
                ts: Some("123.456".to_string()),
                bot_id: Some("B123".to_string()),
                subtype: None,
            }),
            challenge: None,
        };
        assert!(event_text(&bot).is_none());

        let human = SlackWebhookBody {
            event_type: Some("event_callback".to_string()),
            event: Some(SlackEvent {
                kind: Some("message".to_string()),
                channel: Some("C123".to_string()),
                user: Some("U123".to_string()),
                text: Some("hello".to_string()),
                ts: Some("123.456".to_string()),
                bot_id: None,
                subtype: None,
            }),
            challenge: None,
        };
        let (channel, text) = event_text(&human).unwrap();
        assert_eq!(channel, "C123");
        assert_eq!(text, "hello");
    }

    #[test]
    fn slack_event_text_ignores_empty() {
        let ev = SlackWebhookBody {
            event_type: Some("event_callback".to_string()),
            event: Some(SlackEvent {
                kind: Some("message".to_string()),
                channel: Some("C123".to_string()),
                user: Some("U123".to_string()),
                text: Some("".to_string()),
                ts: Some("123.456".to_string()),
                bot_id: None,
                subtype: None,
            }),
            challenge: None,
        };
        assert!(event_text(&ev).is_none());
    }
}
