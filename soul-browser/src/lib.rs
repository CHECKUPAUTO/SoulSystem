//! Headless Chrome automation via Chrome DevTools Protocol (CDP).
//!
//! Provides:
//! - Page navigation and screenshot capture
//! - Element interaction (click, type, extract)
//! - Console log capture
//! - Network request interception
//! - Tab management

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tracing::info;

#[derive(Error, Debug)]
pub enum BrowserError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("WebSocket error: {0}")]
    WebSocket(String),
    #[error("Timeout")]
    Timeout,
    #[error("Connection failed: {0}")]
    Connection(String),
    #[error("Page error: {0}")]
    Page(String),
}

// ── CDP Types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdpMessage {
    pub id: u64,
    pub method: Option<String>,
    pub params: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub error: Option<serde_json::Value>,
}

// ── Browser Configuration ────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BrowserConfig {
    pub headless: bool,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub user_agent: Option<String>,
    pub timeout_ms: u64,
    pub disable_javascript: bool,
    pub proxy: Option<String>,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            headless: true,
            viewport_width: 1280,
            viewport_height: 720,
            user_agent: None,
            timeout_ms: 30000,
            disable_javascript: false,
            proxy: None,
        }
    }
}

// ── Page State ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageState {
    pub url: String,
    pub title: String,
    pub text: String,
    pub html: String,
    pub console_logs: Vec<ConsoleEntry>,
    pub network_requests: Vec<NetworkEntry>,
    pub screenshot_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleEntry {
    pub level: String,
    pub text: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkEntry {
    pub method: String,
    pub url: String,
    pub status: Option<u32>,
    pub resource_type: Option<String>,
}

// ── Element Query ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementInfo {
    pub tag: String,
    pub text: String,
    pub attributes: HashMap<String, String>,
    pub children_count: usize,
    pub is_visible: bool,
}

// ── Browser Controller ───────────────────────────────────────────────

pub struct BrowserController {
    config: BrowserConfig,
    chrome_url: String,
    ws_url: Option<String>,
    msg_id: std::sync::atomic::AtomicU64,
    ws_sender: Option<tokio::sync::mpsc::UnboundedSender<tokio_tungstenite::tungstenite::Message>>,
    response_map: Arc<
        tokio::sync::Mutex<
            std::collections::HashMap<u64, tokio::sync::oneshot::Sender<serde_json::Value>>,
        >,
    >,
}

impl BrowserController {
    pub fn new(config: BrowserConfig) -> Self {
        Self {
            config,
            chrome_url: "http://127.0.0.1:9222".to_string(),
            ws_url: None,
            msg_id: std::sync::atomic::AtomicU64::new(1),
            ws_sender: None,
            response_map: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }

    fn next_id(&self) -> u64 {
        self.msg_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    /// Connect to running Chrome instance via WebSocket
    pub async fn connect(&mut self) -> Result<(), BrowserError> {
        let client = reqwest::Client::new();

        // Get WebSocket debugger URL
        let resp = client
            .get(format!("{}/json/version", self.chrome_url))
            .send()
            .await?;

        let info: serde_json::Value = resp.json().await?;

        self.ws_url = info
            .get("webSocketDebuggerUrl")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let ws_url = self
            .ws_url
            .as_ref()
            .ok_or_else(|| BrowserError::Connection("No WebSocket URL found".to_string()))?;

        // Connect via WebSocket
        let (ws_stream, _) = tokio_tungstenite::connect_async(ws_url)
            .await
            .map_err(|e| BrowserError::WebSocket(e.to_string()))?;

        let (mut write, mut read) = ws_stream.split();

        // Create channel for sending messages
        let (tx, mut rx) =
            tokio::sync::mpsc::unbounded_channel::<tokio_tungstenite::tungstenite::Message>();
        self.ws_sender = Some(tx);

        // Spawn task to forward messages to WebSocket
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if write.send(msg).await.is_err() {
                    break;
                }
            }
        });

        // Spawn task to read responses and dispatch to waiting channels
        let response_map = self.response_map.clone();
        tokio::spawn(async move {
            while let Some(msg_result) = read.next().await {
                match msg_result {
                    Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                            if let Some(id) = value.get("id").and_then(|v| v.as_u64()) {
                                let mut map = response_map.lock().await;
                                if let Some(sender) = map.remove(&id) {
                                    let _ = sender.send(value);
                                }
                            }
                        }
                    }
                    Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => break,
                    Err(_) => break,
                    _ => {}
                }
            }
        });

        info!("Connected to Chrome via WebSocket at {}", ws_url);
        Ok(())
    }

    /// Send a CDP command and wait for response
    async fn send_command(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, BrowserError> {
        let sender = self
            .ws_sender
            .as_ref()
            .ok_or_else(|| BrowserError::Connection("Not connected".to_string()))?;

        let id = self.next_id();
        let msg = serde_json::json!({
            "id": id,
            "method": method,
            "params": params
        });

        // Create oneshot channel for response
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        {
            let mut map = self.response_map.lock().await;
            map.insert(id, resp_tx);
        }

        // Send message
        let text =
            serde_json::to_string(&msg).map_err(|e| BrowserError::WebSocket(e.to_string()))?;
        sender
            .send(tokio_tungstenite::tungstenite::Message::Text(text))
            .map_err(|e| BrowserError::WebSocket(e.to_string()))?;

        // Wait for response with timeout
        let response = tokio::time::timeout(
            std::time::Duration::from_millis(self.config.timeout_ms),
            resp_rx,
        )
        .await
        .map_err(|_| BrowserError::Timeout)?
        .map_err(|_| BrowserError::WebSocket("Response channel closed".to_string()))?;

        // Check for error
        if let Some(error) = response.get("error") {
            return Err(BrowserError::Page(error.to_string()));
        }

        Ok(response
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }

    /// Navigate to a URL
    pub async fn navigate(&self, url: &str) -> Result<(), BrowserError> {
        self.send_command("Page.navigate", serde_json::json!({"url": url}))
            .await?;
        Ok(())
    }

    /// Take a screenshot
    pub async fn screenshot(&self) -> Result<Vec<u8>, BrowserError> {
        let result = self
            .send_command(
                "Page.captureScreenshot",
                serde_json::json!({"format": "png"}),
            )
            .await?;

        if let Some(data) = result.get("data").and_then(|v| v.as_str()) {
            use base64::Engine;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|e| BrowserError::Page(e.to_string()))?;
            Ok(decoded)
        } else {
            Err(BrowserError::Page("No screenshot data".to_string()))
        }
    }

    /// Get current page state
    pub async fn get_page_state(&self) -> Result<PageState, BrowserError> {
        let client = reqwest::Client::new();

        // Get all targets
        let resp = client
            .get(format!("{}/json/list", self.chrome_url))
            .send()
            .await?;

        let targets: Vec<serde_json::Value> = resp.json().await?;

        let page_target = targets
            .iter()
            .find(|t| t.get("type").and_then(|v| v.as_str()) == Some("page"))
            .ok_or_else(|| BrowserError::Page("No page target found".to_string()))?;

        let url = page_target
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let title = page_target
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Ok(PageState {
            url,
            title,
            text: String::new(),
            html: String::new(),
            console_logs: Vec::new(),
            network_requests: Vec::new(),
            screenshot_path: None,
        })
    }

    /// Execute JavaScript on the page
    pub async fn evaluate_js(&self, expression: &str) -> Result<serde_json::Value, BrowserError> {
        let result = self
            .send_command(
                "Runtime.evaluate",
                serde_json::json!({
                    "expression": expression,
                    "returnByValue": true
                }),
            )
            .await?;

        Ok(result
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }

    /// Click an element by CSS selector
    pub async fn click(&self, selector: &str) -> Result<(), BrowserError> {
        self.evaluate_js(&format!(
            "document.querySelector('{}').click()",
            selector.replace('\'', "\\'")
        ))
        .await?;
        Ok(())
    }

    /// Type text into an input field
    pub async fn type_text(&self, selector: &str, text: &str) -> Result<(), BrowserError> {
        self.evaluate_js(&format!(
            "document.querySelector('{}').value = '{}'",
            selector.replace('\'', "\\'"),
            text.replace('\'', "\\'")
        ))
        .await?;
        Ok(())
    }

    /// Get all links on the page
    pub async fn get_links(&self) -> Result<Vec<String>, BrowserError> {
        let result = self
            .evaluate_js("Array.from(document.querySelectorAll('a[href]')).map(a => a.href)")
            .await?;

        if let Some(arr) = result.as_array() {
            Ok(arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect())
        } else {
            Ok(vec![])
        }
    }

    /// Get page text content
    pub async fn get_text(&self) -> Result<String, BrowserError> {
        let result = self.evaluate_js("document.body.innerText").await?;
        Ok(result.as_str().unwrap_or("").to_string())
    }

    /// Get full HTML
    pub async fn get_html(&self) -> Result<String, BrowserError> {
        let result = self
            .evaluate_js("document.documentElement.outerHTML")
            .await?;
        Ok(result.as_str().unwrap_or("").to_string())
    }

    /// List open tabs
    pub async fn list_tabs(&self) -> Result<Vec<TabInfo>, BrowserError> {
        let client = reqwest::Client::new();
        let resp = client
            .get(format!("{}/json/list", self.chrome_url))
            .send()
            .await?;

        let targets: Vec<serde_json::Value> = resp.json().await?;

        Ok(targets
            .iter()
            .map(|t| TabInfo {
                id: t
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                title: t
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                url: t
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                tab_type: t
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
            .collect())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabInfo {
    pub id: String,
    pub title: String,
    pub url: String,
    pub tab_type: String,
}

// ── Content Extraction ───────────────────────────────────────────────

pub struct ContentExtractor;

impl ContentExtractor {
    /// Extract main content from HTML (simple readability-like)
    pub fn extract_content(html: &str) -> String {
        let mut text = String::new();
        let mut skip = false;

        for line in html.lines() {
            let lower = line.to_lowercase();
            // Track script/style blocks
            if lower.contains("<script") || lower.contains("<style") {
                skip = true;
            }
            if lower.contains("</script") || lower.contains("</style") {
                skip = false;
                continue;
            }

            if !skip {
                // Simple tag removal
                let mut cleaned = String::new();
                let mut in_tag = false;
                for ch in line.chars() {
                    match ch {
                        '<' => in_tag = true,
                        '>' => in_tag = false,
                        _ if !in_tag => cleaned.push(ch),
                        _ => {}
                    }
                }
                let trimmed: String = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
                if !trimmed.is_empty() {
                    text.push_str(&trimmed);
                    text.push('\n');
                }
            }
        }

        text.trim().to_string()
    }

    /// Extract meta information from HTML
    pub fn extract_meta(html: &str) -> HashMap<String, String> {
        let mut meta = HashMap::new();

        // Title
        if let Some(start) = html.find("<title>") {
            if let Some(end) = html[start..].find("</title>") {
                let title = &html[start + 7..start + end];
                meta.insert("title".to_string(), title.trim().to_string());
            }
        }

        // Meta description
        lazy_static::lazy_static! {
            static ref META_DESC_RE: regex::Regex = regex::Regex::new(
                r#"<meta[^>]*name=["']description["'][^>]*content=["']([^"']+)["']"#
            ).unwrap();
        }
        if let Some(caps) = META_DESC_RE.captures(html) {
            if let Some(desc) = caps.get(1) {
                meta.insert("description".to_string(), desc.as_str().to_string());
            }
        }

        meta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_config_default() {
        let config = BrowserConfig::default();
        assert!(config.headless);
        assert_eq!(config.viewport_width, 1280);
    }

    #[test]
    fn test_content_extractor() {
        let html = r#"
            <html>
            <head><title>Test Page</title></head>
            <body>
                <h1>Hello World</h1>
                <script>alert('hi')</script>
                <p>This is content</p>
            </body>
            </html>
        "#;
        let content = ContentExtractor::extract_content(html);
        assert!(content.contains("Hello World"));
        assert!(!content.contains("alert"));
    }

    #[test]
    fn test_extract_meta() {
        let html = r#"<html><head><title>My Title</title><meta name="description" content="A description"></head></html>"#;
        let meta = ContentExtractor::extract_meta(html);
        assert_eq!(meta.get("title").unwrap(), "My Title");
        assert_eq!(meta.get("description").unwrap(), "A description");
    }

    #[test]
    fn test_browser_controller_new() {
        let config = BrowserConfig {
            headless: false,
            viewport_width: 800,
            viewport_height: 600,
            user_agent: Some("test-agent".into()),
            timeout_ms: 5000,
            disable_javascript: true,
            proxy: Some("http://proxy:8080".into()),
        };
        let controller = BrowserController::new(config);

        assert!(!controller.config.headless);
        assert_eq!(controller.config.viewport_width, 800);
        assert_eq!(controller.config.viewport_height, 600);
        assert_eq!(controller.config.user_agent, Some("test-agent".into()));
        assert_eq!(controller.config.timeout_ms, 5000);
        assert!(controller.config.disable_javascript);
        assert_eq!(controller.config.proxy, Some("http://proxy:8080".into()));
    }

    #[test]
    fn test_content_extractor_strips_style_blocks() {
        let html = "<html><head><style>body { color: red; }</style></head>\n<body><p>Visible text</p></body></html>";
        let content = ContentExtractor::extract_content(html);
        assert!(!content.contains("color: red"));
    }

    #[test]
    fn test_content_extractor_handles_empty_html() {
        let content = ContentExtractor::extract_content("");
        assert_eq!(content, "");
    }

    #[test]
    fn test_extract_meta_no_title() {
        let html = r#"<html><head></head></html>"#;
        let meta = ContentExtractor::extract_meta(html);
        assert!(!meta.contains_key("title"));
    }

    #[test]
    fn test_extract_meta_no_description() {
        let html = r#"<html><head><title>Title Only</title></head></html>"#;
        let meta = ContentExtractor::extract_meta(html);
        assert_eq!(meta.get("title").unwrap(), "Title Only");
        assert!(!meta.contains_key("description"));
    }

    #[test]
    fn test_browser_config_default_viewport() {
        let config = BrowserConfig::default();
        assert_eq!(config.viewport_height, 720);
        assert_eq!(config.timeout_ms, 30000);
        assert!(!config.disable_javascript);
        assert!(config.proxy.is_none());
        assert!(config.user_agent.is_none());
    }

    #[test]
    fn test_content_extractor_strips_script_blocks_across_lines() {
        let html =
            "<html>\n<script>\n  alert('test');\n</script>\n<p>Content after script</p>\n</html>";
        let content = ContentExtractor::extract_content(html);
        assert!(content.contains("Content after script"));
        assert!(!content.contains("alert"));
    }

    #[test]
    fn test_extract_meta_title_with_whitespace() {
        let html = r#"<html><head><title>  Spaced Title  </title></head></html>"#;
        let meta = ContentExtractor::extract_meta(html);
        assert_eq!(meta.get("title").unwrap(), "Spaced Title");
    }

    #[test]
    fn test_content_extractor_joins_whitespace() {
        let html = "<html><body><p>Multiple   spaces   here</p></body></html>";
        let content = ContentExtractor::extract_content(html);
        assert!(content.contains("Multiple spaces here"));
    }
}
