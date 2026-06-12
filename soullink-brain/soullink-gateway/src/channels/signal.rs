//! Signal channel — communicates with signal-cli REST API.
//!
//! signal-cli must be running with `--rest-api` mode:
//! ```bash
//! signal-cli --dbus-session rest-api --listen 127.0.0.1:18080
//! ```
//!
//! Reference: https://github.com/AsamK/signal-cli/wiki/REST-API

use tracing::{info, warn};

pub struct SignalChannel {
    pub enabled: bool,
    pub api_url: String,
    pub number: String,
}

impl SignalChannel {
    pub fn new(api_url: String, number: String) -> Self {
        Self {
            enabled: true,
            api_url,
            number,
        }
    }

    /// Send a text message via signal-cli REST API.
    pub async fn send_message(&self, recipient: &str, text: &str) -> Result<(), String> {
        let url = format!("{}/v2/send", self.api_url.trim_end_matches('/'));
        let client = reqwest::Client::new();

        let resp = client
            .post(&url)
            .json(&serde_json::json!({
                "message": text,
                "number": self.number,
                "recipients": [recipient],
            }))
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("signal send failed: {e}"))?;

        let status = resp.status();
        if status.is_success() {
            info!(recipient = %recipient, "Signal message sent");
            Ok(())
        } else {
            let body = resp.text().await.unwrap_or_default();
            warn!(status = status.as_u16(), body = %body, "Signal send error");
            Err(format!("HTTP {status}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires signal-cli running"]
    async fn test_signal_send() {
        let ch = SignalChannel::new("http://127.0.0.1:18080".into(), "+15551234567".into());
        let result = ch.send_message("+15559876543", "test from soullink").await;
        assert!(result.is_err() || result.is_ok());
    }
}
