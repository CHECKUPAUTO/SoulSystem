//! Signal integration through signal-cli's JSON-RPC HTTP daemon.

use super::{emit_event, ChannelConfig, ChannelProvider};
use crate::EntityEvent;
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct SignalProvider {
    base_url: Option<String>,
    account: Option<String>,
}

impl SignalProvider {
    pub fn from_env() -> Self {
        Self {
            base_url: std::env::var("SIGNAL_CLI_HTTP_URL")
                .ok()
                .filter(|value| !value.is_empty()),
            account: std::env::var("SIGNAL_ACCOUNT")
                .ok()
                .filter(|value| !value.is_empty()),
        }
    }

    pub fn configured(base_url: impl Into<String>, account: Option<String>) -> Self {
        Self {
            base_url: Some(base_url.into()),
            account,
        }
    }

    fn base_url(&self) -> Result<&str, Box<dyn std::error::Error + Send + Sync>> {
        self.base_url
            .as_deref()
            .ok_or_else(|| "SIGNAL_CLI_HTTP_URL not set".into())
    }

    fn client(&self) -> Result<reqwest::Client, Box<dyn std::error::Error + Send + Sync>> {
        Ok(reqwest::Client::builder()
            .timeout(Duration::from_secs(45))
            .build()?)
    }

    fn send_body(&self, recipient: &str, text: &str) -> Value {
        let mut params = json!({
            "recipient": [recipient],
            "message": text,
        });
        if let Some(account) = &self.account {
            params["account"] = Value::String(account.clone());
        }
        json!({
            "jsonrpc": "2.0",
            "id": uuid::Uuid::new_v4().to_string(),
            "method": "send",
            "params": params,
        })
    }

    async fn receive_loop(self, cfg: ChannelConfig) {
        loop {
            if let Err(error) = self.receive_once(&cfg).await {
                tracing::warn!("signal receive stream interrupted: {error}");
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }

    async fn receive_once(
        &self,
        cfg: &ChannelConfig,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = self.client()?;
        let response = client
            .get(format!(
                "{}/api/v1/events",
                self.base_url()?.trim_end_matches('/')
            ))
            .send()
            .await?
            .error_for_status()?;
        let mut stream = response.bytes_stream();
        let mut pending = String::new();

        while let Some(chunk) = stream.next().await {
            pending.push_str(&String::from_utf8_lossy(&chunk?));
            while let Some(end) = pending.find('\n') {
                let line = pending[..end].trim_end_matches('\r').to_string();
                pending.drain(..=end);
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let event: Value = match serde_json::from_str(data.trim()) {
                    Ok(event) => event,
                    Err(error) => {
                        tracing::debug!("ignored malformed signal event: {error}");
                        continue;
                    }
                };
                let Some((sender, text)) = incoming_text(&event) else {
                    continue;
                };
                emit_event(
                    &cfg.event_bus,
                    EntityEvent::Observation {
                        text: format!("signal:{sender}: {text}"),
                        ts: chrono::Utc::now(),
                    },
                );
                let reply = cfg
                    .entity
                    .ask(text)
                    .await
                    .unwrap_or_else(|error| format!("Error: {error}"));
                if let Err(error) = self.send_message(sender, &reply).await {
                    tracing::warn!("cannot send Signal reply: {error}");
                }
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl ChannelProvider for SignalProvider {
    fn name(&self) -> &'static str {
        "signal"
    }

    async fn start(
        &self,
        cfg: ChannelConfig,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = self.client()?;
        client
            .get(format!(
                "{}/api/v1/check",
                self.base_url()?.trim_end_matches('/')
            ))
            .send()
            .await?
            .error_for_status()?;
        let provider = self.clone();
        tokio::spawn(async move { provider.receive_loop(cfg).await });
        Ok(())
    }

    async fn send_message(
        &self,
        chat_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let response = self
            .client()?
            .post(format!(
                "{}/api/v1/rpc",
                self.base_url()?.trim_end_matches('/')
            ))
            .json(&self.send_body(chat_id, text))
            .send()
            .await?
            .error_for_status()?;
        let body: Value = response.json().await?;
        if let Some(error) = body.get("error") {
            return Err(format!("signal-cli JSON-RPC error: {error}").into());
        }
        Ok(())
    }
}

fn incoming_text(event: &Value) -> Option<(&str, &str)> {
    let params = event.get("params")?;
    let envelope = params
        .pointer("/envelope")
        .or_else(|| params.pointer("/result/envelope"))?;
    let sender = envelope
        .get("sourceNumber")
        .or_else(|| envelope.get("source"))
        .and_then(Value::as_str)?;
    let text = envelope
        .pointer("/dataMessage/message")
        .and_then(Value::as_str)?;
    if text.is_empty() {
        return None;
    }
    Some((sender, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_signal_cli_receive_notification() {
        let event = json!({
            "params": {
                "envelope": {
                    "sourceNumber": "+33123456789",
                    "dataMessage": {"message": "bonjour"}
                }
            }
        });
        assert_eq!(incoming_text(&event), Some(("+33123456789", "bonjour")));
    }

    #[test]
    fn multi_account_send_includes_account() {
        let provider =
            SignalProvider::configured("http://127.0.0.1:8080", Some("+33987654321".into()));
        let body = provider.send_body("+33123456789", "hello");
        assert_eq!(body["method"], "send");
        assert_eq!(body["params"]["account"], "+33987654321");
    }
}
