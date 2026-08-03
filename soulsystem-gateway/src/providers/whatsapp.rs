//! WhatsApp Cloud API provider (Meta Business Platform).
//!
//! Uses the official Meta WhatsApp Cloud API for sending/receiving messages.
//! No headless browser or Node.js bridge required.
//!
//! ## Configuration
//!
//! ```env
//! WHATSAPP_PHONE_NUMBER_ID=123456789
//! WHATSAPP_ACCESS_TOKEN=EAAx...
//! WHATSAPP_WEBHOOK_VERIFY_TOKEN=my_secret
//! WHATSAPP_ALLOWLIST=+336...,+417...  # comma-separated
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, error, info, warn};

// ── Configuration ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WhatsappConfig {
    pub phone_number_id: String,
    pub access_token: String,
    pub webhook_verify_token: String,
    pub api_version: String,
    pub base_url: String,
    pub allowlist: Vec<String>,
    pub self_chat_mode: bool,
    pub auto_reconnect: bool,
}

impl Default for WhatsappConfig {
    fn default() -> Self {
        Self {
            phone_number_id: String::new(),
            access_token: String::new(),
            webhook_verify_token: String::new(),
            api_version: "v22.0".into(),
            base_url: "https://graph.facebook.com".into(),
            allowlist: vec![],
            self_chat_mode: true,
            auto_reconnect: true,
        }
    }
}

impl WhatsappConfig {
    pub fn from_env() -> Option<Self> {
        let phone_number_id = std::env::var("WHATSAPP_PHONE_NUMBER_ID").ok()?;
        let access_token = std::env::var("WHATSAPP_ACCESS_TOKEN").ok()?;
        let webhook_verify_token = std::env::var("WHATSAPP_WEBHOOK_VERIFY_TOKEN")
            .unwrap_or_else(|_| "soulsystem_verify".into());

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
            phone_number_id,
            access_token,
            webhook_verify_token,
            allowlist,
            ..Default::default()
        })
    }
}

// ── Data Types ─────────────────────────────────────────────────────

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
    pub data: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WhatsappStatus {
    pub state: String,
    pub qr: Option<String>,
    pub info: Option<WhatsappConnectionInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WhatsappConnectionInfo {
    pub wid: String,
    pub platform: String,
    pub pushname: Option<String>,
}

// ── WhatsApp Cloud API Types ───────────────────────────────────────

#[derive(Debug, Serialize)]
struct CloudTextMessage {
    messaging_product: String,
    recipient_type: String,
    to: String,
    #[serde(rename = "type")]
    msg_type: String,
    text: TextBody,
}

#[derive(Debug, Serialize)]
struct TextBody {
    body: String,
    preview_url: bool,
}

#[derive(Debug, Serialize)]
struct CloudMediaMessage {
    messaging_product: String,
    recipient_type: String,
    to: String,
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(flatten)]
    media: MediaPayload,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", content = "content")]
enum MediaPayload {
    #[serde(rename = "image")]
    Image { image: MediaContent },
    #[serde(rename = "document")]
    Document { document: MediaContent },
    #[serde(rename = "audio")]
    Audio { audio: MediaContent },
    #[serde(rename = "video")]
    Video { video: MediaContent },
}

#[derive(Debug, Serialize)]
struct MediaContent {
    id: Option<String>,
    link: Option<String>,
    caption: Option<String>,
    filename: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CloudApiResponse {
    messaging_product: String,
    contacts: Vec<ContactResponse>,
    messages: Vec<MessageResponse>,
}

#[derive(Debug, Deserialize)]
struct ContactResponse {
    input: String,
    wa_id: String,
}

#[derive(Debug, Deserialize)]
struct MessageResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct CloudApiError {
    error: CloudApiErrorDetail,
}

#[derive(Debug, Deserialize)]
struct CloudApiErrorDetail {
    message: String,
    #[serde(rename = "type")]
    error_type: String,
    code: i32,
    error_subcode: Option<i32>,
    fbtrace_id: String,
}

// ── Webhook Incoming Payload ───────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct WebhookPayload {
    pub object: String,
    pub entry: Vec<WebhookEntry>,
}

#[derive(Debug, Deserialize)]
pub struct WebhookEntry {
    pub id: String,
    pub changes: Vec<WebhookChange>,
}

#[derive(Debug, Deserialize)]
pub struct WebhookChange {
    pub value: WebhookValue,
    pub field: String,
}

#[derive(Debug, Deserialize)]
pub struct WebhookValue {
    pub messaging_product: Option<String>,
    pub metadata: Option<WebhookMetadata>,
    pub contacts: Option<Vec<WebhookContact>>,
    pub messages: Option<Vec<WebhookIncomingMessage>>,
    pub statuses: Option<Vec<WebhookStatus>>,
}

#[derive(Debug, Deserialize)]
pub struct WebhookMetadata {
    pub display_phone_number: String,
    pub phone_number_id: String,
}

#[derive(Debug, Deserialize)]
pub struct WebhookContact {
    pub wa_id: String,
    pub profile: Option<WebhookProfile>,
}

#[derive(Debug, Deserialize)]
pub struct WebhookProfile {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct WebhookIncomingMessage {
    pub from: String,
    pub id: String,
    pub timestamp: String,
    #[serde(rename = "type")]
    pub msg_type: Option<String>,
    pub text: Option<WebhookTextBody>,
    pub image: Option<WebhookMediaBody>,
    pub document: Option<WebhookMediaBody>,
    pub audio: Option<WebhookMediaBody>,
    pub video: Option<WebhookMediaBody>,
}

#[derive(Debug, Deserialize)]
pub struct WebhookTextBody {
    pub body: String,
}

#[derive(Debug, Deserialize)]
pub struct WebhookMediaBody {
    pub id: Option<String>,
    pub mime_type: Option<String>,
    pub sha256: Option<String>,
    pub filename: Option<String>,
    pub caption: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WebhookStatus {
    pub id: String,
    pub status: String,
    pub timestamp: String,
    pub recipient_id: String,
    pub conversation: Option<WebhookConversation>,
}

#[derive(Debug, Deserialize)]
pub struct WebhookConversation {
    pub id: String,
    pub origin: Option<WebhookOrigin>,
}

#[derive(Debug, Deserialize)]
pub struct WebhookOrigin {
    #[serde(rename = "type")]
    pub origin_type: String,
}

// ── Provider Implementation ────────────────────────────────────────

pub struct WhatsappProvider {
    config: WhatsappConfig,
    http_client: reqwest::Client,
    status: WhatsappStatus,
    message_tx: tokio::sync::mpsc::Sender<WhatsappMessage>,
    message_rx: tokio::sync::mpsc::Receiver<WhatsappMessage>,
}

impl WhatsappProvider {
    pub fn new(config: WhatsappConfig) -> Self {
        let (message_tx, message_rx) = tokio::sync::mpsc::channel(100);

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self {
            config,
            http_client,
            status: WhatsappStatus {
                state: "disconnected".into(),
                qr: None,
                info: None,
            },
            message_tx,
            message_rx,
        }
    }

    /// Build API URL for a given endpoint.
    fn api_url(&self, endpoint: &str) -> String {
        format!(
            "{}/{}/{}/{}",
            self.config.base_url, self.config.api_version, self.config.phone_number_id, endpoint
        )
    }

    /// Send a message via the WhatsApp Cloud API.
    async fn api_send<T: Serialize + std::fmt::Debug>(
        &self,
        payload: &T,
    ) -> Result<CloudApiResponse, String> {
        let url = self.api_url("messages");

        let response = self
            .http_client
            .post(&url)
            .bearer_auth(&self.config.access_token)
            .json(payload)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "unreadable body".into());

        if status.is_success() {
            serde_json::from_str::<CloudApiResponse>(&body)
                .map_err(|e| format!("Failed to parse response: {e}"))
        } else {
            // Try to parse error body
            if let Ok(err) = serde_json::from_str::<CloudApiError>(&body) {
                Err(format!(
                    "API error {}: {} (code {})",
                    err.error.code, err.error.message, err.error.error_type
                ))
            } else {
                Err(format!("HTTP {}: {}", status.as_u16(), body))
            }
        }
    }

    /// Upload media to WhatsApp via the media upload endpoint.
    async fn upload_media(
        &self,
        data: &[u8],
        mime_type: &str,
    ) -> Result<String, String> {
        let url = format!(
            "{}/{}/{}/media",
            self.config.base_url, self.config.api_version, self.config.phone_number_id
        );

        let part = reqwest::multipart::Part::bytes(data.to_vec())
            .file_name(format!("file.{}", mime_type.split('/').last().unwrap_or("bin")))
            .mime_str(mime_type)
            .map_err(|e| format!("MIME error: {e}"))?;

        let form = reqwest::multipart::Form::new().part("file", part);

        let response = self
            .http_client
            .post(&url)
            .bearer_auth(&self.config.access_token)
            .multipart(form)
            .send()
            .await
            .map_err(|e| format!("Media upload failed: {e}"))?;

        let body = response.text().await.unwrap_or_default();

        #[derive(Deserialize)]
        struct MediaUploadResponse {
            id: String,
        }

        serde_json::from_str::<MediaUploadResponse>(&body)
            .map(|r| r.id)
            .map_err(|e| format!("Failed to parse media upload response: {e}"))
    }

    /// Check if a phone number is in the allowlist.
    pub fn is_allowed(&self, phone: &str) -> bool {
        if self.config.allowlist.is_empty() {
            return self.config.self_chat_mode;
        }

        let normalized = Self::normalize_phone(phone);
        self.config
            .allowlist
            .iter()
            .any(|allowed| Self::normalize_phone(allowed) == normalized)
    }

    fn normalize_phone(phone: &str) -> String {
        phone
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '+')
            .collect()
    }

    // ── Public API ──

    /// Initialize the provider. Verifies the API token is valid.
    pub async fn start(&mut self) -> anyhow::Result<()> {
        if self.config.phone_number_id.is_empty() || self.config.access_token.is_empty() {
            info!(
                "WhatsApp Cloud API not configured (missing WHATSAPP_PHONE_NUMBER_ID or WHATSAPP_ACCESS_TOKEN)"
            );
            self.status.state = "unconfigured".into();
            return Ok(());
        }

        info!(
            "Starting WhatsApp Cloud API provider (phone_number_id={})...",
            self.config.phone_number_id
        );

        // Validate connectivity by making a simple request
        let url = format!(
            "{}/{}/{}",
            self.config.base_url, self.config.api_version, self.config.phone_number_id
        );
        match self.http_client.get(&url).bearer_auth(&self.config.access_token).send().await {
            Ok(resp) if resp.status().is_success() => {
                self.status = WhatsappStatus {
                    state: "ready".into(),
                    qr: None,
                    info: Some(WhatsappConnectionInfo {
                        wid: self.config.phone_number_id.clone(),
                        platform: "cloud_api".into(),
                        pushname: None,
                    }),
                };
                info!("WhatsApp Cloud API connected (phone_number_id={})", self.config.phone_number_id);
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                warn!("WhatsApp Cloud API connectivity check failed: HTTP {} — {}", status.as_u16(), body);
                self.status.state = "auth_error".into();
                return Err(anyhow::anyhow!("WhatsApp API connectivity check failed: {}", body));
            }
            Err(e) => {
                warn!("WhatsApp Cloud API unreachable: {}", e);
                self.status.state = "network_error".into();
                return Err(anyhow::anyhow!("WhatsApp API unreachable: {}", e));
            }
        }

        Ok(())
    }

    /// Stop the provider.
    pub async fn stop(&mut self) {
        info!("Stopping WhatsApp Cloud API provider...");
        self.status.state = "disconnected".into();
    }

    /// Send a text message via the WhatsApp Cloud API.
    pub async fn send_text(&self, to: &str, body: &str) -> anyhow::Result<()> {
        if !self.is_connected() {
            return Err(anyhow::anyhow!("WhatsApp not connected (state: {})", self.status.state));
        }

        if !self.is_allowed(to) {
            return Err(anyhow::anyhow!("Phone number not in allowlist: {}", to));
        }

        let payload = CloudTextMessage {
            messaging_product: "whatsapp".into(),
            recipient_type: "individual".into(),
            to: to.to_string(),
            msg_type: "text".into(),
            text: TextBody {
                body: body.to_string(),
                preview_url: true,
            },
        };

        self.api_send(&payload).await.map_err(|e| {
            error!("Failed to send WhatsApp text to {}: {}", to, e);
            anyhow::anyhow!(e)
        })?;

        debug!("WhatsApp text sent to {}", to);
        Ok(())
    }

    /// Send a media message (image, document, audio, video).
    pub async fn send_media(
        &self,
        to: &str,
        caption: &str,
        mime_type: &str,
        data: &[u8],
        filename: Option<&str>,
    ) -> anyhow::Result<()> {
        if !self.is_connected() {
            return Err(anyhow::anyhow!("WhatsApp not connected (state: {})", self.status.state));
        }

        if !self.is_allowed(to) {
            return Err(anyhow::anyhow!("Phone number not in allowlist: {}", to));
        }

        // Upload media first to get media ID
        let media_id = self.upload_media(data, mime_type).await.map_err(|e| {
            error!("WhatsApp media upload failed: {}", e);
            anyhow::anyhow!(e)
        })?;

        let (msg_type, media_content) = if mime_type.starts_with("image/") {
            (
                "image",
                MediaContent {
                    id: Some(media_id),
                    link: None,
                    caption: if caption.is_empty() { None } else { Some(caption.to_string()) },
                    filename: filename.map(|f| f.to_string()),
                },
            )
        } else if mime_type.starts_with("video/") {
            (
                "video",
                MediaContent {
                    id: Some(media_id),
                    link: None,
                    caption: if caption.is_empty() { None } else { Some(caption.to_string()) },
                    filename: filename.map(|f| f.to_string()),
                },
            )
        } else if mime_type.starts_with("audio/") {
            (
                "audio",
                MediaContent {
                    id: Some(media_id),
                    link: None,
                    caption: None,
                    filename: filename.map(|f| f.to_string()),
                },
            )
        } else {
            (
                "document",
                MediaContent {
                    id: Some(media_id),
                    link: None,
                    caption: if caption.is_empty() { None } else { Some(caption.to_string()) },
                    filename: Some(filename.unwrap_or("attachment").to_string()),
                },
            )
        };

        let payload = CloudMediaMessage {
            messaging_product: "whatsapp".into(),
            recipient_type: "individual".into(),
            to: to.to_string(),
            msg_type: msg_type.into(),
            media: match msg_type {
                "image" => MediaPayload::Image {
                    image: media_content,
                },
                "document" => MediaPayload::Document {
                    document: media_content,
                },
                "audio" => MediaPayload::Audio {
                    audio: media_content,
                },
                _ => MediaPayload::Video {
                    video: media_content,
                },
            },
        };

        self.api_send(&payload).await.map_err(|e| {
            error!("Failed to send WhatsApp media to {}: {}", to, e);
            anyhow::anyhow!(e)
        })?;

        debug!("WhatsApp media sent to {}", to);
        Ok(())
    }

    /// Check if connected and ready.
    pub fn is_connected(&self) -> bool {
        self.status.state == "ready"
    }

    /// Get current connection status.
    pub fn status(&self) -> &WhatsappStatus {
        &self.status
    }

    /// Verify a webhook challenge from Meta.
    pub fn verify_webhook(&self, mode: &str, token: &str, challenge: &str) -> Option<String> {
        if mode == "subscribe" && token == self.config.webhook_verify_token {
            Some(challenge.to_string())
        } else {
            None
        }
    }

    /// Process an incoming webhook payload from Meta.
    /// Extracts messages and pushes them to the message channel.
    pub fn process_webhook(&self, payload: &WebhookPayload) -> Vec<WhatsappMessage> {
        let mut messages = Vec::new();

        for entry in &payload.entry {
            for change in &entry.changes {
                let value = &change.value;

                // Process incoming messages
                if let Some(msgs) = &value.messages {
                    for msg in msgs {
                        let from = msg.from.clone();
                        let to = value
                            .metadata
                            .as_ref()
                            .map(|m| m.display_phone_number.clone())
                            .unwrap_or_default();

                        if !self.is_allowed(&from) {
                            debug!("Ignoring WhatsApp message from non-allowlisted: {}", from);
                            continue;
                        }

                        let body = extract_message_body(msg);

                        let wa_msg = WhatsappMessage {
                            id: msg.id.clone(),
                            from,
                            to,
                            body,
                            timestamp: msg
                                .timestamp
                                .parse()
                                .unwrap_or(chrono::Utc::now().timestamp()),
                            has_media: msg.image.is_some()
                                || msg.document.is_some()
                                || msg.audio.is_some()
                                || msg.video.is_some(),
                            is_group: false,
                            media_info: extract_media_info(msg),
                        };

                        let _ = self.message_tx.try_send(wa_msg.clone());
                        messages.push(wa_msg);
                    }
                }

                // Process status updates
                if let Some(statuses) = &value.statuses {
                    for _status in statuses {
                        debug!(
                            "WhatsApp message status: {} -> {}",
                            _status.id, _status.status
                        );
                    }
                }
            }
        }

        messages
    }

    /// Receive next message from the channel (non-blocking poll).
    pub fn try_receive(&mut self) -> Option<WhatsappMessage> {
        self.message_rx.try_recv().ok()
    }

    /// Receive next message (async, blocking).
    pub async fn receive(&mut self) -> Option<WhatsappMessage> {
        self.message_rx.recv().await
    }
}

// ── Helpers ────────────────────────────────────────────────────────

fn extract_message_body(msg: &WebhookIncomingMessage) -> String {
    if let Some(text) = &msg.text {
        return text.body.clone();
    }
    if let Some(img) = &msg.image {
        return img.caption.clone().unwrap_or_else(|| "[image]".into());
    }
    if let Some(doc) = &msg.document {
        return doc.caption.clone().unwrap_or_else(|| "[document]".into());
    }
    if msg.audio.is_some() {
        return "[audio]".into();
    }
    if let Some(vid) = &msg.video {
        return vid.caption.clone().unwrap_or_else(|| "[video]".into());
    }
    "[unknown message type]".into()
}

fn extract_media_info(msg: &WebhookIncomingMessage) -> Option<MediaInfo> {
    let media = msg.image.as_ref()
        .or(msg.document.as_ref())
        .or(msg.audio.as_ref())
        .or(msg.video.as_ref())?;

    Some(MediaInfo {
        mimetype: media.mime_type.clone().unwrap_or_else(|| "application/octet-stream".into()),
        filename: media.filename.clone(),
        data: None,
        url: media.id.clone().map(|id| {
            format!("https://graph.facebook.com/v22.0/{}/", id)
        }),
    })
}
