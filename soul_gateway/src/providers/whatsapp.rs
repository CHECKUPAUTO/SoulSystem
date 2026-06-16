//! # WhatsApp Provider
//!
//! Webhook-based WhatsApp Business integration. Requires:
//!
//! - `WHATSAPP_ACCESS_TOKEN` — permanent token for the WhatsApp Business API.
//! - `WHATSAPP_PHONE_NUMBER_ID` — ID of the registered phone number.
//! - `WHATSAPP_WEBHOOK_SECRET` — optional app secret for signature verification.
//!
//! Endpoint exposed by the HTTP server:
//! `POST /providers/whatsapp/webhook`

use super::{emit_event, ChannelConfig, ChannelProvider};
use crate::EntityEvent;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub struct WhatsAppProvider {
    token: Option<String>,
    phone_number_id: Option<String>,
    #[allow(dead_code)]
    webhook_secret: Option<String>,
}

impl WhatsAppProvider {
    pub fn from_env() -> Self {
        Self {
            token: std::env::var("WHATSAPP_ACCESS_TOKEN").ok(),
            phone_number_id: std::env::var("WHATSAPP_PHONE_NUMBER_ID").ok(),
            webhook_secret: std::env::var("WHATSAPP_WEBHOOK_SECRET").ok(),
        }
    }

    fn client(&self) -> Result<reqwest::Client, Box<dyn std::error::Error + Send + Sync>> {
        Ok(reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?)
    }
}

#[async_trait::async_trait]
impl ChannelProvider for WhatsAppProvider {
    fn name(&self) -> &'static str {
        "whatsapp"
    }

    async fn start(
        &self,
        cfg: ChannelConfig,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let token = self.token.clone().ok_or("WHATSAPP_ACCESS_TOKEN not set")?;
        let phone_id = self
            .phone_number_id
            .clone()
            .ok_or("WHATSAPP_PHONE_NUMBER_ID not set")?;
        let _ = cfg;
        if token.is_empty() || phone_id.is_empty() {
            return Err("WHATSAPP_ACCESS_TOKEN or WHATSAPP_PHONE_NUMBER_ID is empty".into());
        }
        Ok(())
    }

    async fn send_message(
        &self,
        chat_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let token = self.token.clone().ok_or("WHATSAPP_ACCESS_TOKEN not set")?;
        let phone_id = self
            .phone_number_id
            .clone()
            .ok_or("WHATSAPP_PHONE_NUMBER_ID not set")?;
        let url = format!("https://graph.facebook.com/v19.0/{phone_id}/messages");
        let body = serde_json::json!({
            "messaging_product": "whatsapp",
            "recipient_type": "individual",
            "to": chat_id,
            "type": "text",
            "text": { "body": text }
        });
        let client = self.client()?;
        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            return Err(format!("WhatsApp API error {status}: {txt}").into());
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WhatsAppWebhookBody {
    pub object: Option<String>,
    pub entry: Option<Vec<WhatsAppEntry>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WhatsAppEntry {
    pub id: Option<String>,
    pub changes: Option<Vec<WhatsAppChange>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WhatsAppChange {
    pub value: Option<WhatsAppValue>,
    pub field: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WhatsAppValue {
    pub messaging_product: Option<String>,
    pub metadata: Option<WhatsAppMetadata>,
    pub contacts: Option<Vec<WhatsAppContact>>,
    pub messages: Option<Vec<WhatsAppMessage>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WhatsAppMetadata {
    pub display_phone_number: Option<String>,
    pub phone_number_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WhatsAppContact {
    pub wa_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WhatsAppMessage {
    pub from: Option<String>,
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub msg_type: Option<String>,
    pub text: Option<WhatsAppText>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WhatsAppText {
    pub body: Option<String>,
}

pub fn first_message_text(body: &WhatsAppWebhookBody) -> Option<(&str, &str)> {
    let entry = body.entry.as_ref()?.first()?;
    let change = entry.changes.as_ref()?.first()?;
    let value = change.value.as_ref()?;
    let msg = value.messages.as_ref()?.first()?;
    if msg.msg_type.as_deref() != Some("text") {
        return None;
    }
    let from = msg.from.as_deref()?;
    let text = msg.text.as_ref()?.body.as_deref()?;
    if text.is_empty() {
        return None;
    }
    Some((from, text))
}

pub async fn handle_webhook(
    entity: &Arc<dyn crate::EntityHandle>,
    bus: &Option<crate::EventHub>,
    body: WhatsAppWebhookBody,
) -> serde_json::Value {
    let reply = match first_message_text(&body) {
        Some((from, text)) => {
            emit_event(
                bus,
                EntityEvent::Observation {
                    text: format!("whatsapp:{from}: {text}"),
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
    fn whatsapp_provider_missing_config_start_fails() {
        let p = WhatsAppProvider {
            token: Some("token".to_string()),
            phone_number_id: None,
            webhook_secret: None,
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
        assert!(err.contains("WHATSAPP_PHONE_NUMBER_ID"));
    }

    #[test]
    fn whatsapp_first_message_text_extracts_text() {
        let body = WhatsAppWebhookBody {
            object: Some("whatsapp".to_string()),
            entry: Some(vec![WhatsAppEntry {
                id: Some("e1".to_string()),
                changes: Some(vec![WhatsAppChange {
                    value: Some(WhatsAppValue {
                        messaging_product: Some("whatsapp".to_string()),
                        metadata: None,
                        contacts: None,
                        messages: Some(vec![WhatsAppMessage {
                            from: Some("15551234567".to_string()),
                            id: Some("msg1".to_string()),
                            msg_type: Some("text".to_string()),
                            text: Some(WhatsAppText {
                                body: Some("hi".to_string()),
                            }),
                        }]),
                    }),
                    field: Some("messages".to_string()),
                }]),
            }]),
        };
        let (from, text) = first_message_text(&body).unwrap();
        assert_eq!(from, "15551234567");
        assert_eq!(text, "hi");
    }

    #[test]
    fn whatsapp_first_message_text_skips_non_text() {
        let body = WhatsAppWebhookBody {
            object: Some("whatsapp".to_string()),
            entry: Some(vec![WhatsAppEntry {
                id: Some("e1".to_string()),
                changes: Some(vec![WhatsAppChange {
                    value: Some(WhatsAppValue {
                        messaging_product: Some("whatsapp".to_string()),
                        metadata: None,
                        contacts: None,
                        messages: Some(vec![WhatsAppMessage {
                            from: Some("15551234567".to_string()),
                            id: Some("msg1".to_string()),
                            msg_type: Some("image".to_string()),
                            text: None,
                        }]),
                    }),
                    field: Some("messages".to_string()),
                }]),
            }]),
        };
        assert!(first_message_text(&body).is_none());
    }
}
