//! soul-dashboard — Web dashboard for SoulSystem monitoring.
//!
//! Provides HTTP endpoints for organ health, bus events, turbulence,
//! and an HTML dashboard UI with WebSocket real-time updates.
//!
//! # Endpoints
//!
//! - `GET /` — HTML dashboard
//! - `GET /api/health` — JSON health summary
//! - `GET /api/organs` — JSON organ list
//! - `GET /api/events` — JSON recent events
//! - `GET /api/turbulence` — JSON turbulence entries
//! - `GET /api/agent` — JSON agent state
//! - `GET /ws` — WebSocket for real-time updates

use axum::{
    extract::{
        connect_info::ConnectInfo,
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

// ── Data Model ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganHealth {
    pub name: String,
    pub healthy: bool,
    pub uptime_s: u64,
    pub vram_used_mib: u64,
    pub vram_total_mib: u64,
    pub avg_latency_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusEvent {
    pub timestamp: String,
    pub kind: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurbulenceEntry {
    pub timestamp: String,
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentState {
    pub turn: usize,
    pub goal: String,
    pub status: String,
    pub tools_used: Vec<String>,
    pub memory_items: usize,
    pub uptime_secs: u64,
    pub last_action: Option<String>,
    pub last_result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DashboardState {
    pub organs: Vec<OrganHealth>,
    pub events: VecDeque<BusEvent>,
    pub turbulence: VecDeque<TurbulenceEntry>,
    pub agent: AgentState,
    pub costs: CostSummary,
}

impl DashboardState {
    pub fn new() -> Self {
        Self {
            agent: AgentState {
                turn: 0,
                goal: String::new(),
                status: "idle".into(),
                tools_used: Vec::new(),
                memory_items: 0,
                uptime_secs: 0,
                last_action: None,
                last_result: None,
            },
            ..Default::default()
        }
    }

    pub fn push_event(&mut self, event: BusEvent) {
        self.events.push_back(event);
        while self.events.len() > 100 {
            self.events.pop_front();
        }
    }

    pub fn push_turbulence(&mut self, entry: TurbulenceEntry) {
        self.turbulence.push_back(entry);
        while self.turbulence.len() > 50 {
            self.turbulence.pop_front();
        }
    }
}

// ── Cost Tracking ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEntry {
    pub task: String,
    pub model: String,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub cost_usd: f64,
    pub latency_ms: u64,
    pub timestamp: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CostSummary {
    pub entries: Vec<CostEntry>,
    pub total_cost_usd: f64,
    pub total_tokens: u32,
    pub by_model: std::collections::HashMap<String, (u32, f64)>,
}

impl CostSummary {
    pub fn add(&mut self, entry: CostEntry) {
        self.total_cost_usd += entry.cost_usd;
        self.total_tokens += entry.tokens_in + entry.tokens_out;
        let model_entry = self.by_model.entry(entry.model.clone()).or_insert((0, 0.0));
        model_entry.0 += entry.tokens_in + entry.tokens_out;
        model_entry.1 += entry.cost_usd;
        self.entries.push(entry);
    }
}

/// Thread-safe wrapper for CostSummary.
#[derive(Debug, Clone)]
pub struct SharedCostSummary {
    inner: std::sync::Arc<tokio::sync::RwLock<CostSummary>>,
}

impl SharedCostSummary {
    pub fn new() -> Self {
        Self {
            inner: std::sync::Arc::new(tokio::sync::RwLock::new(CostSummary::default())),
        }
    }

    pub async fn add(&self, entry: CostEntry) {
        self.inner.write().await.add(entry);
    }

    pub async fn snapshot(&self) -> CostSummary {
        self.inner.read().await.clone()
    }
}

impl Default for SharedCostSummary {
    fn default() -> Self {
        Self::new()
    }
}

// ── Rate Limiter ─────────────────────────────────────────────────────────

/// Simple token-bucket rate limiter keyed by IP.
pub struct SimpleRateLimiter {
    buckets: HashMap<String, Vec<std::time::Instant>>,
    max_requests: usize,
    window: std::time::Duration,
}

impl SimpleRateLimiter {
    pub fn new(max_requests: usize, window_secs: u64) -> Self {
        Self {
            buckets: HashMap::new(),
            max_requests,
            window: std::time::Duration::from_secs(window_secs),
        }
    }

    /// Returns `true` if the request is allowed, `false` if rate-limited.
    pub fn check(&mut self, key: &str) -> bool {
        let now = std::time::Instant::now();
        let timestamps = self.buckets.entry(key.to_string()).or_default();
        timestamps.retain(|t| now.duration_since(*t) < self.window);
        if timestamps.len() >= self.max_requests {
            false
        } else {
            timestamps.push(now);
            true
        }
    }
}

impl Default for SimpleRateLimiter {
    fn default() -> Self {
        Self::new(60, 60)
    }
}

// ── Shared App State ────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub dashboard: Arc<RwLock<DashboardState>>,
    pub event_tx: broadcast::Sender<BusEvent>,
    pub auth_token: Option<String>,
    pub rate_limiter: std::sync::Arc<std::sync::Mutex<SimpleRateLimiter>>,
}

// ── Auth ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct WsParams {
    pub token: Option<String>,
}

fn check_auth(token: Option<&str>, expected: Option<&str>) -> bool {
    match expected {
        None => true, // No auth required
        Some(expected_token) => match token {
            Some(t) => t == expected_token,
            None => false,
        },
    }
}

// ── Handlers ────────────────────────────────────────────────────────────

async fn index() -> Html<&'static str> {
    Html(include_str!("dashboard.html"))
}

async fn health_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let s = state.dashboard.read().await;
    let total = s.organs.len();
    let healthy = s.organs.iter().filter(|o| o.healthy).count();

    Ok(Json(serde_json::json!({
        "status": if healthy == total { "healthy" } else { "degraded" },
        "organs_total": total,
        "organs_healthy": healthy,
        "events_count": s.events.len(),
        "turbulence_count": s.turbulence.len(),
    })))
}

async fn organs_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<OrganHealth>>, StatusCode> {
    let s = state.dashboard.read().await;
    Ok(Json(s.organs.clone()))
}

async fn events_handler(State(state): State<AppState>) -> Result<Json<Vec<BusEvent>>, StatusCode> {
    let s = state.dashboard.read().await;
    Ok(Json(s.events.iter().rev().take(50).cloned().collect()))
}

async fn turbulence_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<TurbulenceEntry>>, StatusCode> {
    let s = state.dashboard.read().await;
    Ok(Json(s.turbulence.iter().rev().take(20).cloned().collect()))
}

async fn agent_handler(State(state): State<AppState>) -> Result<Json<AgentState>, StatusCode> {
    let s = state.dashboard.read().await;
    Ok(Json(s.agent.clone()))
}

async fn costs_handler(State(state): State<AppState>) -> Result<Json<CostSummary>, StatusCode> {
    let s = state.dashboard.read().await;
    Ok(Json(s.costs.clone()))
}

// ── WebSocket Handler ───────────────────────────────────────────────────

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WsParams>,
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    if !check_auth(params.token.as_deref(), state.auth_token.as_deref()) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    let ip = addr.ip().to_string();
    {
        let mut limiter = state.rate_limiter.lock().unwrap();
        if !limiter.check(&ip) {
            return (StatusCode::TOO_MANY_REQUESTS, "Rate limited").into_response();
        }
    }
    ws.on_upgrade(move |socket| handle_socket(socket, state, ip))
}

async fn handle_socket(socket: WebSocket, state: AppState, client_ip: String) {
    let (mut sender, mut receiver) = socket.split();

    // Send initial state
    {
        let s = state.dashboard.read().await;
        let init = DashboardEvent {
            event_type: "init".into(),
            data: serde_json::json!({
                "organs": s.organs,
                "agent": s.agent,
                "events": s.events.iter().rev().take(20).cloned().collect::<Vec<_>>(),
                "turbulence": s.turbulence.iter().rev().take(10).cloned().collect::<Vec<_>>(),
            }),
        };

        if let Ok(json) = serde_json::to_string(&init) {
            let _ = sender.send(Message::Text(json.into())).await;
        }
    }

    // Subscribe to broadcast channel for real-time updates
    let mut rx = state.event_tx.subscribe();

    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(_t))) => {
                        let allowed = {
                            let mut limiter = state.rate_limiter.lock().unwrap();
                            limiter.check(&client_ip)
                        };
                        if !allowed {
                            let err = serde_json::json!({"error": "rate_limited"});
                            if let Ok(json) = serde_json::to_string(&err) {
                                let _ = sender.send(Message::Text(json.into())).await;
                            }
                        }
                    }
                    Some(Ok(Message::Binary(_d))) => {
                        let allowed = {
                            let mut limiter = state.rate_limiter.lock().unwrap();
                            limiter.check(&client_ip)
                        };
                        if !allowed {
                            let err = serde_json::json!({"error": "rate_limited"});
                            if let Ok(json) = serde_json::to_string(&err) {
                                let _ = sender.send(Message::Text(json.into())).await;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            Ok(event) = rx.recv() => {
                let update = DashboardEvent {
                    event_type: "event".into(),
                    data: serde_json::json!({
                        "timestamp": event.timestamp,
                        "kind": event.kind,
                        "source": event.source,
                    }),
                };

                if let Ok(json) = serde_json::to_string(&update) {
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(30)) => {
                let heartbeat = DashboardEvent {
                    event_type: "heartbeat".into(),
                    data: serde_json::json!({"ts": chrono::Utc::now().to_rfc3339()}),
                };
                if let Ok(json) = serde_json::to_string(&heartbeat) {
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    }
}

// ── Server ──────────────────────────────────────────────────────────────

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/health", get(health_handler))
        .route("/api/organs", get(organs_handler))
        .route("/api/events", get(events_handler))
        .route("/api/turbulence", get(turbulence_handler))
        .route("/api/agent", get(agent_handler))
        .route("/api/costs", get(costs_handler))
        .route("/ws", get(ws_handler))
        .with_state(state)
        .layer(tower_http::cors::CorsLayer::permissive())
}

pub async fn run_server(
    dashboard: Arc<RwLock<DashboardState>>,
    event_tx: broadcast::Sender<BusEvent>,
    port: u16,
    auth_token: Option<String>,
) -> anyhow::Result<()> {
    let state = AppState {
        dashboard,
        event_tx,
        auth_token,
        rate_limiter: std::sync::Arc::new(std::sync::Mutex::new(SimpleRateLimiter::default())),
    };
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    tracing::info!("Dashboard listening on 127.0.0.1:{}", port);
    axum::serve(
        listener,
        app(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> AppState {
        let (tx, _) = broadcast::channel(16);
        AppState {
            dashboard: Arc::new(RwLock::new(DashboardState::default())),
            event_tx: tx,
            auth_token: None,
            rate_limiter: std::sync::Arc::new(std::sync::Mutex::new(SimpleRateLimiter::default())),
        }
    }

    #[tokio::test]
    async fn test_health_handler() {
        let state = test_state();
        let response = health_handler(State(state)).await.unwrap();
        let json: serde_json::Value = serde_json::from_str(&response.0.to_string()).unwrap();
        assert_eq!(json["status"], "healthy");
    }

    #[tokio::test]
    async fn test_organs_handler() {
        let state = test_state();
        let Json(organs) = organs_handler(State(state)).await.unwrap();
        assert!(organs.is_empty());
    }

    #[tokio::test]
    async fn test_agent_handler() {
        let state = test_state();
        let Json(agent) = agent_handler(State(state)).await.unwrap();
        assert!(agent.status.is_empty() || agent.status == "idle");
        assert_eq!(agent.turn, 0);
    }

    #[tokio::test]
    async fn test_push_event() {
        let mut state = DashboardState::default();
        let event = BusEvent {
            timestamp: "2026-06-08T12:00:00Z".into(),
            kind: "test".into(),
            source: "test".into(),
        };
        state.push_event(event);
        assert_eq!(state.events.len(), 1);
    }

    #[tokio::test]
    async fn test_push_turbulence() {
        let mut state = DashboardState::default();
        let entry = TurbulenceEntry {
            timestamp: "2026-06-08T12:00:00Z".into(),
            severity: "low".into(),
            message: "test".into(),
        };
        state.push_turbulence(entry);
        assert_eq!(state.turbulence.len(), 1);
    }

    #[test]
    fn test_auth_check() {
        assert!(check_auth(None, None)); // No auth required
        assert!(check_auth(Some("abc"), Some("abc"))); // Correct token
        assert!(!check_auth(Some("wrong"), Some("abc"))); // Wrong token
        assert!(!check_auth(None, Some("abc"))); // Missing token
    }
}
