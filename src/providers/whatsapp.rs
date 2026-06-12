use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

/// WhatsApp Web.js provider configuration
#[derive(Debug, Clone)]
pub struct WhatsappConfig {
    pub session_name: String,
    pub data_path: String,
    pub puppeteer_headless: bool,
    pub puppeteer_executable_path: Option<String>,
    pub self_chat_mode: bool,
    pub allowlist: Vec<String>, // Phone numbers in international format
    pub auto_reconnect: bool,
    pub qr_timeout_secs: u64,
}

impl Default for WhatsappConfig {
    fn default() -> Self {
        Self {
            session_name: "openclaw".into(),
            data_path: "/var/lib/openclaw/whatsapp".into(),
            puppeteer_headless: true,
            puppeteer_executable_path: None,
            self_chat_mode: true,
            allowlist: vec![],
            auto_reconnect: true,
            qr_timeout_secs: 60,
        }
    }
}

impl WhatsappConfig {
    pub fn from_env() -> Option<Self> {
        let session_name = std::env::var("WHATSAPP_SESSION_NAME")
            .unwrap_or_else(|_| "openclaw".into());
        
        let allowlist = std::env::var("WHATSAPP_ALLOWLIST")
            .ok()
            .map(|s| {
                s.split(',')
                    .map(|n| n.trim().to_string())
                    .filter(|n| !n.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        Some(Self {
            session_name,
            data_path: std::env::var("WHATSAPP_DATA_PATH")
                .unwrap_or_else(|_| "/var/lib/openclaw/whatsapp".into()),
            puppeteer_headless: !matches!(
                std::env::var("WHATSAPP_HEADFUL").ok().as_deref(),
                Some("1" | "true" | "yes")
            ),
            puppeteer_executable_path: std::env::var("PUPPETEER_EXECUTABLE_PATH").ok(),
            self_chat_mode: matches!(
                std::env::var("WHATSAPP_SELF_CHAT").ok().as_deref(),
                Some("1" | "true" | "yes" | "on") | None // Default to true
            ),
            allowlist,
            auto_reconnect: matches!(
                std::env::var("WHATSAPP_AUTO_RECONNECT").ok().as_deref(),
                Some("1" | "true" | "yes") | None // Default to true
            ),
            qr_timeout_secs: std::env::var("WHATSAPP_QR_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(60),
        })
    }
}

/// WhatsApp message
#[derive(Debug, Clone, Deserialize)]
pub struct WhatsappMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub body: String,
    pub timestamp: i64,
    pub has_media: bool,
    pub is_group: bool,
    #[serde(default)]
    pub media_info: Option<MediaInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MediaInfo {
    pub mimetype: String,
    pub filename: Option<String>,
    pub data: Option<String>, // base64 encoded
    pub url: Option<String>,
}

/// WhatsApp status update
#[derive(Debug, Clone, Deserialize)]
pub struct WhatsappStatus {
    pub state: String, // "connected", "disconnected", "qr", "loading", "ready"
    pub qr: Option<String>,
    pub info: Option<WhatsappConnectionInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WhatsappConnectionInfo {
    pub wid: String,
    pub platform: String,
    pub pushname: Option<String>,
}

/// Message to send via WhatsApp
#[derive(Debug, Clone, Serialize)]
pub struct WhatsappSendMessage {
    pub to: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<SendOptions>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SendOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quoted_message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaAttachment>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaAttachment {
    pub mimetype: String,
    pub data: String, // base64 encoded
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

/// WhatsApp Web.js bridge interface
/// 
/// This is a Rust wrapper around the WhatsApp Web.js library running via
/// a Node.js bridge process. The actual implementation uses puppeteer to
/// control a headless browser with WhatsApp Web.
pub struct WhatsappProvider {
    config: WhatsappConfig,
    bridge_process: Option<tokio::process::Child>,
    status: WhatsappStatus,
    message_tx: tokio::sync::mpsc::Sender<WhatsappMessage>,
    message_rx: tokio::sync::mpsc::Receiver<WhatsappMessage>,
}

impl WhatsappProvider {
    pub fn new(config: WhatsappConfig) -> Self {
        let (message_tx, message_rx) = tokio::sync::mpsc::channel(100);
        
        Self {
            config,
            bridge_process: None,
            status: WhatsappStatus {
                state: "disconnected".into(),
                qr: None,
                info: None,
            },
            message_tx,
            message_rx,
        }
    }

    /// Check if phone number is in allowlist
    pub fn is_allowed(&self, phone: &str) -> bool {
        if self.config.allowlist.is_empty() && !self.config.self_chat_mode {
            return false;
        }
        
        let normalized = Self::normalize_phone(phone);
        
        if self.config.self_chat_mode {
            // In self-chat mode, only allow the owner's number
            // This would typically come from config
            return true; // Placeholder - actual implementation needs config
        }
        
        self.config.allowlist.iter().any(|allowed| {
            Self::normalize_phone(allowed) == normalized
        })
    }

    fn normalize_phone(phone: &str) -> String {
        phone.chars()
            .filter(|c| c.is_ascii_digit() || *c == '+')
            .collect()
    }

    /// Start the WhatsApp bridge process
    pub async fn start(&mut self) -> anyhow::Result<()> {
        info!("Starting WhatsApp Web.js bridge...");
        
        // TODO: Implement Node.js bridge process management
        // This would spawn a Node.js process running whatsapp-web.js
        // and communicate via stdin/stdout JSON protocol
        
        warn!("WhatsApp bridge not yet implemented - placeholder only");
        Ok(())
    }

    /// Stop the WhatsApp bridge process
    pub async fn stop(&mut self) {
        if let Some(mut process) = self.bridge_process.take() {
            info!("Stopping WhatsApp bridge process...");
            let _ = process.kill().await;
        }
        self.status.state = "disconnected".into();
    }

    /// Send a text message
    pub async fn send_text(&self, to: &str, body: &str) -> anyhow::Result<()> {
        if !self.is_connected() {
            return Err(anyhow::anyhow!("WhatsApp not connected"));
        }
        
        if !self.is_allowed(to) {
            return Err(anyhow::anyhow!("Phone number not in allowlist: {}", to));
        }

        let msg = WhatsappSendMessage {
            to: to.to_string(),
            body: body.to_string(),
            options: None,
        };

        // TODO: Send to bridge process via stdin
        debug!("Sending message to {}: {}", to, body);
        Ok(())
    }

    /// Send media message
    pub async fn send_media(
        &self,
        to: &str,
        body: &str,
        mimetype: &str,
        data: &[u8],
        filename: Option<&str>,
    ) -> anyhow::Result<()> {
        if !self.is_connected() {
            return Err(anyhow::anyhow!("WhatsApp not connected"));
        }

        let base64_data = base64::encode(data);
        
        let msg = WhatsappSendMessage {
            to: to.to_string(),
            body: body.to_string(),
            options: Some(SendOptions {
                quoted_message_id: None,
                media: Some(MediaAttachment {
                    mimetype: mimetype.to_string(),
                    data: base64_data,
                    filename: filename.map(|f| f.to_string()),
                }),
            }),
        };

        // TODO: Send to bridge process
        debug!("Sending media message to {}", to);
        Ok(())
    }

    /// Check if connected to WhatsApp
    pub fn is_connected(&self) -> bool {
        self.status.state == "ready"
    }

    /// Get current status
    pub fn status(&self) -> &WhatsappStatus {
        &self.status
    }

    /// Handle status update from bridge
    fn handle_status_update(&mut self, status: WhatsappStatus) {
        match status.state.as_str() {
            "qr" => {
                info!("WhatsApp QR code ready for scanning");
                if let Some(ref qr) = status.qr {
                    info!("QR: {}...", &qr[..qr.len().min(20)]);
                }
            }
            "ready" => {
                if let Some(ref info) = status.info {
                    info!("WhatsApp connected as {} ({:?})", info.wid, info.pushname);
                }
            }
            "disconnected" => {
                warn!("WhatsApp disconnected");
                if self.config.auto_reconnect {
                    // TODO: Trigger reconnect
                }
            }
            _ => {}
        }
        
        self.status = status;
    }

    /// Process incoming message from bridge
    fn handle_message(&self, message: WhatsappMessage) {
        if !self.is_allowed(&message.from) {
            debug!("Ignoring message from non-allowlisted number: {}", message.from);
            return;
        }

        let _ = self.message_tx.try_send(message);
    }

    /// Receive next message (if any)
    pub async fn receive(&mut self) -> Option<WhatsappMessage> {
        self.message_rx.recv().await
    }
}

impl Drop for WhatsappProvider {
    fn drop(&mut self) {
        if self.bridge_process.is_some() {
            // Cannot await in drop, just kill the process
            // In production, use a proper shutdown mechanism
        }
    }
}

use base64;
