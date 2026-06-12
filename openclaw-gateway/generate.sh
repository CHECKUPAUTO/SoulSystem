#!/usr/bin/env bash
set -e

############################################
# OPENCLAW GATEWAY GENERATOR
# Ce script génère le projet Rust complet
############################################

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "🦀 Génération du projet OpenClaw Gateway..."

############################################
# STRUCTURE DU PROJET
############################################
mkdir -p src/providers
cd "$SCRIPT_DIR"

############################################
# Cargo.toml
############################################
cat > Cargo.toml <<'EOF'
[package]
name = "openclaw-gateway"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1.44", features = ["full"] }
axum = { version = "0.8", features = ["ws"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["trace"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
dashmap = "6.1"
uuid = { version = "1.10", features = ["serde", "v4"] }
chrono = { version = "0.4", features = ["serde", "clock"] }
once_cell = "1.19"
anyhow = "1.0"
thiserror = "1.0"
async-trait = "0.1"
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }
futures = "0.3"
futures-util = "0.3"
base64 = "0.22"
EOF

echo "✅ Cargo.toml créé"

############################################
# src/main.rs
############################################
cat > src/main.rs <<'EOF'
use tracing::info;

mod config;
mod gateway;
mod protocol;
mod session;
mod auth;
mod providers;

use config::GatewayConfig;
use gateway::GatewayServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = GatewayConfig::from_env();

    tracing_subscriber::fmt()
        .with_env_filter(config.log_level.clone())
        .init();

    std::panic::set_hook(Box::new(|panic| {
        tracing::error!("Panic occurred: {:?}", panic);
    }));

    info!("🦀 OpenClaw Gateway Rust v{} starting...", env!("CARGO_PKG_VERSION"));
    info!("Configuration: port={}, workers={}", config.port, config.workers);

    let server = GatewayServer::new(config);
    server.run().await?;

    Ok(())
}
EOF

echo "✅ src/main.rs créé"

############################################
# src/config.rs
############################################
cat > src/config.rs <<'EOF'
use std::env;

#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub port: u16,
    pub workers: usize,
    pub auth_token: Option<String>,
    pub log_level: String,
    pub enable_telegram: bool,
    pub enable_whatsapp: bool,
    pub telegram_token: Option<String>,
    pub whatsapp_session: Option<String>,
}

fn env_bool(key: &str) -> bool {
    matches!(env::var(key).ok().as_deref(), Some("1" | "true" | "yes" | "on"))
}

impl GatewayConfig {
    pub fn from_env() -> Self {
        let port = env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(18889);
        let workers = env::var("WORKERS").ok().and_then(|w| w.parse().ok()).unwrap_or(4).clamp(1, 64);

        let cfg = Self {
            port,
            workers,
            auth_token: env::var("GATEWAY_TOKEN").ok(),
            log_level: env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
            enable_telegram: env_bool("ENABLE_TELEGRAM"),
            enable_whatsapp: env_bool("ENABLE_WHATSAPP"),
            telegram_token: env::var("TELEGRAM_TOKEN").ok(),
            whatsapp_session: env::var("WHATSAPP_SESSION").ok(),
        };

        if cfg.auth_token.is_none() {
            tracing::warn!("⚠️ GATEWAY_TOKEN not set — gateway running without static operator token");
        }

        cfg
    }
}
EOF

echo "✅ src/config.rs créé"

############################################
# src/protocol.rs
############################################
cat > src/protocol.rs <<'EOF'
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: u16 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Message {
    #[serde(rename = "req")]
    Request(Request),
    #[serde(rename = "res")]
    Response(Response),
    #[serde(rename = "event")]
    Event(Event),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: String,
    pub ok: bool,
    #[serde(flatten)]
    pub payload: ResponsePayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponsePayload {
    Success { payload: Value },
    Error { error: ErrorPayload },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub event: String,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectRequest {
    pub min_protocol: u16,
    pub max_protocol: u16,
    pub client: ClientInfo,
    pub role: Role,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub caps: Vec<String>,
    pub auth: AuthInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloOk {
    pub typ: String,
    pub protocol: u16,
    pub session_id: String,
    pub device_token: String,
    pub policy: PolicyInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyInfo {
    pub heartbeat_interval_ms: u64,
    pub max_message_size: usize,
    pub idle_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub id: String,
    pub version: String,
    pub platform: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Operator,
    Node,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

pub fn error_response(id: &str, code: &str, message: &str) -> Response {
    Response {
        id: id.to_string(),
        ok: false,
        payload: ResponsePayload::Error {
            error: ErrorPayload {
                code: code.to_string(),
                message: message.to_string(),
                details: None,
            },
        },
    }
}

pub fn success_response(id: &str, payload: Value) -> Response {
    Response {
        id: id.to_string(),
        ok: true,
        payload: ResponsePayload::Success { payload },
    }
}
EOF

echo "✅ src/protocol.rs créé"

############################################
# src/auth.rs
############################################
cat > src/auth.rs <<'EOF'
use crate::protocol::Role;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub token: String,
    pub role: Role,
    pub scopes: Vec<String>,
    pub created_at_ms: i64,
    pub expires_at_ms: Option<i64>,
    pub client_id: String,
}

pub struct AuthManager {
    tokens: Arc<DashMap<String, TokenInfo>>,
    config_token: Option<String>,
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64
}

impl AuthManager {
    pub fn new(config_token: Option<String>) -> Self {
        Self {
            tokens: Arc::new(DashMap::new()),
            config_token,
        }
    }

    pub fn validate_token(&self, token: &str) -> Option<TokenInfo> {
        if let Some(ref cfg) = self.config_token {
            if token == cfg {
                return Some(TokenInfo {
                    token: token.to_string(),
                    role: Role::Operator,
                    scopes: vec!["operator.admin".into()],
                    created_at_ms: now_ms(),
                    expires_at_ms: None,
                    client_id: "config".into(),
                });
            }
        }

        let entry = self.tokens.get(token)?;
        if let Some(exp) = entry.expires_at_ms {
            if now_ms() > exp {
                self.tokens.remove(token);
                return None;
            }
        }

        Some(entry.clone())
    }

    pub fn generate_device_token(&self, role: Role, scopes: Vec<String>, client_id: String) -> TokenInfo {
        let token = format!("oc_dev_{}", Uuid::new_v4().as_simple());
        let now = now_ms();
        let expires = now + 30 * 24 * 60 * 60 * 1000;

        let info = TokenInfo {
            token: token.clone(),
            role,
            scopes,
            created_at_ms: now,
            expires_at_ms: Some(expires),
            client_id,
        };

        self.tokens.insert(token.clone(), info.clone());
        info!("Generated device token {}", token);
        info
    }

    pub fn cleanup_expired_tokens(&self) {
        let now = now_ms();
        let expired: Vec<_> = self
            .tokens
            .iter()
            .filter(|e| e.value().expires_at_ms.map(|x| x < now).unwrap_or(false))
            .map(|e| e.key().clone())
            .collect();

        for t in expired {
            self.tokens.remove(&t);
            debug!("Removed expired token {}", t);
        }
    }
}
EOF

echo "✅ src/auth.rs créé"

############################################
# src/session.rs
############################################
cat > src/session.rs <<'EOF'
use crate::protocol::Role;
use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use tokio::sync::{RwLock, mpsc};
use tracing::{info, warn};
use uuid::Uuid;

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64
}

pub type SessionId = String;

#[derive(Debug, Clone)]
pub struct Session {
    pub id: SessionId,
    pub client_id: String,
    pub role: Role,
    pub scopes: Vec<String>,
    pub caps: Vec<String>,
    pub connected_at_ms: i64,
    pub last_activity_ms: Arc<RwLock<i64>>,
    pub event_tx: Option<mpsc::UnboundedSender<Value>>,
}

impl Session {
    pub fn new(client_id: String, role: Role) -> Self {
        let now = now_ms();
        Self {
            id: Uuid::new_v4().to_string(),
            client_id,
            role,
            scopes: vec![],
            caps: vec![],
            connected_at_ms: now,
            last_activity_ms: Arc::new(RwLock::new(now)),
            event_tx: None,
        }
    }

    pub async fn update_activity(&self) {
        *self.last_activity_ms.write().await = now_ms();
    }

    pub async fn is_stale(&self, timeout: Duration) -> bool {
        now_ms() - *self.last_activity_ms.read().await > timeout.as_millis() as i64
    }
}

pub struct SessionManager {
    pub sessions: Arc<DashMap<SessionId, Arc<Session>>>,
    by_client_id: Arc<DashMap<String, SessionId>>,
    stale_timeout: Duration,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
            by_client_id: Arc::new(DashMap::new()),
            stale_timeout: Duration::from_secs(300),
        }
    }

    pub fn insert(&self, session: Arc<Session>) {
        if let Some(old) = self.by_client_id.insert(session.client_id.clone(), session.id.clone()) {
            self.sessions.remove(&old);
            info!("Replaced old session {}", old);
        }
        self.sessions.insert(session.id.clone(), session.clone());
    }

    pub async fn cleanup_stale_sessions(&self) {
        let stale: Vec<_> = {
            let sessions = self.sessions.clone();
            let mut stale = Vec::new();
            for entry in sessions.iter() {
                let session = Arc::clone(entry.value());
                if session.is_stale(self.stale_timeout).await {
                    stale.push(entry.key().clone());
                }
            }
            stale
        };

        for id in stale {
            self.sessions.remove(&id);
            warn!("Removed stale session {}", id);
        }
    }

    pub fn start_cleanup_task(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                self.cleanup_stale_sessions().await;
            }
        });
    }
}
EOF

echo "✅ src/session.rs créé"

############################################
# src/gateway.rs
############################################
cat > src/gateway.rs <<'EOF'
use crate::auth::AuthManager;
use crate::config::GatewayConfig;
use crate::protocol::{
    ConnectRequest, HelloOk, PolicyInfo, Message, Request, Response,
    PROTOCOL_VERSION, error_response, success_response,
};
use crate::session::{Session, SessionManager};
use axum::{
    extract::{State, WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use axum::extract::ws::{Message as WsMessage, WebSocket};
use futures_util::sink::SinkExt;
use futures_util::stream::StreamExt;
use serde_json::json;
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tracing::info;

#[derive(Clone)]
pub struct GatewayState {
    pub config: GatewayConfig,
    pub auth: Arc<AuthManager>,
    pub sessions: Arc<SessionManager>,
}

pub struct GatewayServer {
    state: GatewayState,
}

impl GatewayServer {
    pub fn new(config: GatewayConfig) -> Self {
        let auth = Arc::new(AuthManager::new(config.auth_token.clone()));
        let sessions = Arc::new(SessionManager::new());

        Self {
            state: GatewayState {
                config,
                auth,
                sessions,
            },
        }
    }

    pub async fn run(self) -> anyhow::Result<()> {
        let state = self.state.clone();

        state.sessions.clone().start_cleanup_task();

        let app = Router::new()
            .route("/ws", get(ws_handler))
            .route("/health", get(health_handler))
            .route("/status", get(status_handler))
            .with_state(state);

        let addr: SocketAddr = format!("0.0.0.0:{}", self.state.config.port).parse()?;
        let listener = TcpListener::bind(addr).await?;

        info!("🚀 Gateway listening on ws://{}/ws", addr);

        axum::serve(listener, app).await?;
        Ok(())
    }
}

async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

async fn status_handler(State(state): State<GatewayState>) -> impl IntoResponse {
    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "sessions": state.sessions.sessions.len(),
        "port": state.config.port,
    }))
}

async fn ws_handler(State(state): State<GatewayState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: GatewayState) {
    let (mut sender, mut receiver) = socket.split();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<Value>();

    let send_task = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            if let Ok(msg) = serde_json::to_string(&event) {
                if sender.send(WsMessage::Text(msg.into())).await.is_err() {
                    break;
                }
            }
        }
    });

    let mut current_session: Option<Arc<Session>> = None;

    while let Some(Ok(msg)) = receiver.next().await {
        if let WsMessage::Text(text) = msg {
            match serde_json::from_str::<Message>(&text) {
                Ok(Message::Request(req)) => {
                    let response = handle_request(&req, &state, &mut current_session, &event_tx).await;
                    if let Ok(val) = serde_json::to_value(&response) {
                        let _ = event_tx.send(val);
                    }
                }
                _ => {}
            }
        }
    }

    if let Some(s) = current_session {
        state.sessions.sessions.remove(&s.id);
    }

    drop(event_tx);
    let _ = send_task.await;
}

async fn handle_request(
    req: &Request,
    state: &GatewayState,
    current_session: &mut Option<Arc<Session>>,
    event_tx: &mpsc::UnboundedSender<Value>,
) -> Response {
    match req.method.as_str() {
        "connect" => {
            let Ok(connect_req) = serde_json::from_value::<ConnectRequest>(req.params.clone()) else {
                return error_response(&req.id, "INVALID_PARAMS", "Invalid connect params");
            };

            if PROTOCOL_VERSION < connect_req.min_protocol || PROTOCOL_VERSION > connect_req.max_protocol {
                return error_response(&req.id, "UNSUPPORTED_PROTOCOL", "Protocol mismatch");
            }

            let Some(token) = connect_req.auth.token else {
                return error_response(&req.id, "AUTH_REQUIRED", "Missing token");
            };

            let Some(token_info) = state.auth.validate_token(&token) else {
                return error_response(&req.id, "AUTH_FAILED", "Invalid token");
            };

            let mut session = Session::new(connect_req.client.id.clone(), connect_req.role.clone());
            session.scopes = connect_req.scopes.clone();
            session.caps = connect_req.caps.clone();
            session.event_tx = Some(event_tx.clone());

            let session = Arc::new(session);
            state.sessions.insert(session.clone());
            *current_session = Some(session.clone());

            let auth_response = state.auth.generate_device_token(connect_req.role, connect_req.scopes, connect_req.client.id);

            let hello = HelloOk {
                typ: "hello-ok".into(),
                protocol: PROTOCOL_VERSION,
                session_id: session.id.clone(),
                device_token: auth_response.token.clone(),
                policy: PolicyInfo {
                    heartbeat_interval_ms: 30000,
                    max_message_size: 65536,
                    idle_timeout_ms: 300000,
                },
            };

            success_response(&req.id, json!(hello))
        }
        _ => error_response(&req.id, "UNKNOWN_METHOD", "Unknown method"),
    }
}
EOF

echo "✅ src/gateway.rs créé"

############################################
# src/providers/mod.rs
############################################
cat > src/providers/mod.rs <<'EOF'
pub mod telegram;
pub mod whatsapp;
EOF

echo "✅ src/providers/mod.rs créé"

echo ""
echo "🎉 Génération terminée !"
echo ""
echo "Pour compiler : cargo build --release"
echo "Pour exécuter  : cargo run"
echo ""
