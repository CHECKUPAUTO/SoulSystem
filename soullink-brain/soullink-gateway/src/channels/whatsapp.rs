//! WhatsApp channel — placeholder for WhatsApp Business API integration.
//!
//! Integration strategies (to be implemented):
//! - WhatsApp Business API (official) via /v1/messages endpoint
//! - whatsapp-web.js (unofficial) via puppeteer
//!
//! This module is a stub with documented integration points.

use tracing::info;

pub struct WhatsAppChannel {
    pub enabled: bool,
    pub api_url: Option<String>,
    pub phone_number_id: Option<String>,
}

impl WhatsAppChannel {
    pub fn new() -> Self {
        Self {
            enabled: false,
            api_url: None,
            phone_number_id: None,
        }
    }

    /// Stub: send message placeholder.
    pub async fn send_message(&self, _recipient: &str, _text: &str) -> Result<(), String> {
        info!("WhatsApp channel not yet implemented");
        Err("WhatsApp channel is a stub — not implemented".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "WhatsApp not implemented"]
    async fn test_whatsapp_send() {
        let ch = WhatsAppChannel::new();
        let result = ch.send_message("+15551234567", "test").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("stub"));
    }
}
