use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

// ═══════════════════════════════════════════════════════════════
// REST API
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn error(message: &str) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.to_string()),
            timestamp: chrono::Utc::now(),
        }
    }
}

pub struct RestApi {
    port: u16,
}

impl RestApi {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Default for RestApi {
    fn default() -> Self {
        Self::new(9030)
    }
}

// ═══════════════════════════════════════════════════════════════
// DASHBOARD
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardData {
    pub status: serde_json::Value,
    pub metrics: serde_json::Value,
    pub alerts: Vec<serde_json::Value>,
    pub recent_activity: Vec<serde_json::Value>,
}

pub struct Dashboard {
    data: DashboardData,
}

impl Dashboard {
    pub fn new() -> Self {
        Self {
            data: DashboardData {
                status: serde_json::json!({}),
                metrics: serde_json::json!({}),
                alerts: Vec::new(),
                recent_activity: Vec::new(),
            },
        }
    }

    pub fn update(&mut self, data: DashboardData) {
        self.data = data;
    }

    pub fn html(&self) -> String {
        format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <title>SoulSystem Dashboard</title>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <style>
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 0; padding: 20px; background: #1a1a2e; color: #eee; }}
        .container {{ max-width: 1200px; margin: 0 auto; }}
        .card {{ background: #16213e; border-radius: 8px; padding: 20px; margin: 10px 0; }}
        .metric {{ display: inline-block; margin: 10px; padding: 15px; background: #0f3460; border-radius: 8px; min-width: 150px; }}
        .metric h3 {{ margin: 0; color: #e94560; }}
        .metric p {{ margin: 5px 0 0; font-size: 24px; }}
        h1 {{ color: #e94560; }}
        .status {{ padding: 5px 10px; border-radius: 4px; font-size: 12px; }}
        .online {{ background: #4caf50; }}
        .offline {{ background: #f44336; }}
    </style>
</head>
<body>
    <div class="container">
        <h1>SoulSystem Dashboard</h1>
        <div class="card">
            <h2>System Status</h2>
            <pre>{}</pre>
        </div>
        <div class="card">
            <h2>Metrics</h2>
            <pre>{}</pre>
        </div>
        <div class="card">
            <h2>Recent Activity</h2>
            <pre>{}</pre>
        </div>
    </div>
</body>
</html>"#,
            serde_json::to_string_pretty(&self.data.status).unwrap(),
            serde_json::to_string_pretty(&self.data.metrics).unwrap(),
            serde_json::to_string_pretty(&self.data.recent_activity).unwrap(),
        )
    }
}

impl Default for Dashboard {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// TELEGRAM BOT
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramMessage {
    pub chat_id: i64,
    pub text: String,
}

pub struct TelegramBot {
    token: Option<String>,
}

impl TelegramBot {
    pub fn new() -> Self {
        Self { token: None }
    }

    pub fn set_token(&mut self, token: &str) {
        self.token = Some(token.to_string());
    }

    pub fn is_configured(&self) -> bool {
        self.token.is_some()
    }

    pub async fn send_message(&self, chat_id: i64, text: &str) -> Result<(), ApiError> {
        if let Some(token) = &self.token {
            let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
            let client = reqwest::Client::new();
            let _ = client.post(&url)
                .json(&serde_json::json!({
                    "chat_id": chat_id,
                    "text": text,
                }))
                .send()
                .await;
            Ok(())
        } else {
            Err(ApiError::BadRequest("Telegram token not configured".into()))
        }
    }
}

impl Default for TelegramBot {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// API ENGINE (ties everything together)
// ═══════════════════════════════════════════════════════════════

pub struct ApiEngine {
    pub rest: RestApi,
    pub dashboard: Dashboard,
    pub telegram: TelegramBot,
}

impl ApiEngine {
    pub fn new() -> Self {
        Self {
            rest: RestApi::new(9030),
            dashboard: Dashboard::new(),
            telegram: TelegramBot::new(),
        }
    }

    pub fn status(&self) -> serde_json::Value {
        serde_json::json!({
            "rest_port": self.rest.port(),
            "telegram_configured": self.telegram.is_configured(),
        })
    }
}

impl Default for ApiEngine {
    fn default() -> Self {
        Self::new()
    }
}
