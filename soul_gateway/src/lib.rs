//! # soul_gateway — Surface de contrôle HTTP/WebSocket
//!
//! Inspiré du gateway openclaw (style `/v1/chat/completions`,
//! `/tools/invoke`, sessions, hooks). Expose l'entité autonome via :
//!
//! - `POST /v1/ask`            — question au LLM
//! - `POST /v1/plan`           — crée un plan pour un objectif
//! - `POST /v1/run`            — exécute une commande via le sandbox
//! - `POST /v1/cycle`          — un cycle cognitif complet
//! - `GET  /v1/status`         — état de l'entité
//! - `GET  /v1/goals`          — liste des objectifs
//! - `WS   /v1/stream`         — flux d'événements temps réel
//! - `GET  /health`            — healthcheck
//!
//! Le routeur est construit autour d'un trait `EntityHandle` que l'entité
//! implémente, ce qui permet de tester le gateway sans dépendre du runtime
//! complet.

use async_trait::async_trait;
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

/// Erreur côté API.
#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("entity error: {0}")]
    Entity(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Événement émis par l'entité, broadcast aux clients WS.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum EntityEvent {
    /// Un nouveau but a été créé.
    GoalCreated { id: String, description: String, ts: DateTime<Utc> },
    /// Un plan a été généré.
    PlanCreated { id: String, goal_id: String, n_steps: usize, ts: DateTime<Utc> },
    /// Une étape a été exécutée.
    StepExecuted { command: String, success: bool, ms: u64, ts: DateTime<Utc> },
    /// Une observation a été ajoutée.
    Observation { text: String, ts: DateTime<Utc> },
    /// Une décision a été prise.
    Decision { action: String, confidence: f32, ts: DateTime<Utc> },
    /// Un cycle a démarré.
    CycleStarted { id: String, ts: DateTime<Utc> },
    /// Un cycle s'est terminé.
    CycleFinished { id: String, success: bool, ts: DateTime<Utc> },
    /// Erreur système.
    Error { message: String, ts: DateTime<Utc> },
    /// Ping périodique pour keepalive.
    Heartbeat { ts: DateTime<Utc> },
}

/// Trait que toute entité autonome doit implémenter pour être pilotable.
#[async_trait]
pub trait EntityHandle: Send + Sync {
    async fn ask(&self, prompt: &str) -> Result<String, String>;
    async fn status(&self) -> serde_json::Value;
    async fn create_goal(&self, description: &str) -> Result<String, String>;
    async fn plan(&self, goal_id: &str) -> Result<Vec<String>, String>;
    async fn execute_plan(&self, goal_id: &str) -> Result<String, String>;
    async fn execute_shell(&self, cmd: &str) -> Result<String, String>;
    async fn run_cycle(&self) -> Result<serde_json::Value, String>;
    async fn list_goals(&self) -> Vec<serde_json::Value>;
}

/// Hub d'événements : tous les clients WS sont notifiés.
#[derive(Clone)]
pub struct EventHub {
    inner: Arc<Mutex<VecDeque<EntityEvent>>>,
    capacity: usize,
}

impl EventHub {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
            capacity,
        }
    }

    pub fn publish(&self, event: EntityEvent) {
        let mut q = self.inner.lock();
        q.push_back(event);
        while q.len() > self.capacity {
            q.pop_front();
        }
    }

    pub fn recent(&self, n: usize) -> Vec<EntityEvent> {
        let q = self.inner.lock();
        let start = q.len().saturating_sub(n);
        q.iter().skip(start).cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().is_empty()
    }
}

/// État partagé entre les handlers.
#[derive(Clone)]
pub struct GatewayState {
    pub entity: Arc<dyn EntityHandle>,
    pub events: EventHub,
}

impl GatewayState {
    pub fn new(entity: Arc<dyn EntityHandle>) -> Self {
        Self {
            entity,
            events: EventHub::new(500),
        }
    }
}

// ── DTOs ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AskRequest {
    pub prompt: String,
}

#[derive(Debug, Serialize)]
pub struct AskResponse {
    pub response: String,
}

#[derive(Debug, Deserialize)]
pub struct GoalRequest {
    pub description: String,
}

#[derive(Debug, Serialize)]
pub struct GoalResponse {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Serialize)]
pub struct PlanResponse {
    pub goal_id: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ExecuteResponse {
    pub result: String,
}

#[derive(Debug, Deserialize)]
pub struct ShellRequest {
    pub command: String,
}

#[derive(Debug, Serialize)]
pub struct CycleResponse {
    pub cycle: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ── Handlers ────────────────────────────────────────────────

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "ts": Utc::now(),
        "service": "soul_gateway",
    }))
}

async fn handle_ask(
    State(st): State<GatewayState>,
    Json(req): Json<AskRequest>,
) -> Result<Json<AskResponse>, (StatusCode, Json<ErrorResponse>)> {
    if req.prompt.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: "prompt vide".into() }),
        ));
    }
    match st.entity.ask(&req.prompt).await {
        Ok(resp) => Ok(Json(AskResponse { response: resp })),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e }))),
    }
}

async fn handle_create_goal(
    State(st): State<GatewayState>,
    Json(req): Json<GoalRequest>,
) -> Result<Json<GoalResponse>, (StatusCode, Json<ErrorResponse>)> {
    if req.description.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: "description vide".into() }),
        ));
    }
    let desc = req.description.clone();
    match st.entity.create_goal(&desc).await {
        Ok(id) => {
            st.events.publish(EntityEvent::GoalCreated {
                id: id.clone(),
                description: desc,
                ts: Utc::now(),
            });
            Ok(Json(GoalResponse { id, description: req.description }))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e }))),
    }
}

async fn handle_plan(
    State(st): State<GatewayState>,
    axum::extract::Path(goal_id): axum::extract::Path<String>,
) -> Result<Json<PlanResponse>, (StatusCode, Json<ErrorResponse>)> {
    match st.entity.plan(&goal_id).await {
        Ok(steps) => {
            st.events.publish(EntityEvent::PlanCreated {
                id: Uuid::new_v4().to_string(),
                goal_id: goal_id.clone(),
                n_steps: steps.len(),
                ts: Utc::now(),
            });
            Ok(Json(PlanResponse { goal_id, steps }))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e }))),
    }
}

async fn handle_execute_plan(
    State(st): State<GatewayState>,
    axum::extract::Path(goal_id): axum::extract::Path<String>,
) -> Result<Json<ExecuteResponse>, (StatusCode, Json<ErrorResponse>)> {
    match st.entity.execute_plan(&goal_id).await {
        Ok(result) => Ok(Json(ExecuteResponse { result })),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e }))),
    }
}

async fn handle_shell(
    State(st): State<GatewayState>,
    Json(req): Json<ShellRequest>,
) -> Result<Json<ExecuteResponse>, (StatusCode, Json<ErrorResponse>)> {
    if req.command.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: "commande vide".into() }),
        ));
    }
    match st.entity.execute_shell(&req.command).await {
        Ok(result) => Ok(Json(ExecuteResponse { result })),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e }))),
    }
}

async fn handle_cycle(
    State(st): State<GatewayState>,
) -> Result<Json<CycleResponse>, (StatusCode, Json<ErrorResponse>)> {
    let cycle_id = Uuid::new_v4().to_string();
    st.events.publish(EntityEvent::CycleStarted { id: cycle_id.clone(), ts: Utc::now() });
    match st.entity.run_cycle().await {
        Ok(cycle) => {
            st.events.publish(EntityEvent::CycleFinished {
                id: cycle_id,
                success: true,
                ts: Utc::now(),
            });
            Ok(Json(CycleResponse { cycle }))
        }
        Err(e) => {
            st.events.publish(EntityEvent::Error { message: e.clone(), ts: Utc::now() });
            st.events.publish(EntityEvent::CycleFinished {
                id: cycle_id,
                success: false,
                ts: Utc::now(),
            });
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e })))
        }
    }
}

async fn handle_status(State(st): State<GatewayState>) -> impl IntoResponse {
    let s = st.entity.status().await;
    Json(s)
}

async fn handle_list_goals(State(st): State<GatewayState>) -> impl IntoResponse {
    Json(st.entity.list_goals().await)
}

async fn handle_recent_events(State(st): State<GatewayState>) -> impl IntoResponse {
    Json(st.events.recent(50))
}

async fn handle_ws(
    State(st): State<GatewayState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| ws_loop(socket, st))
}

async fn ws_loop(mut socket: WebSocket, state: GatewayState) {
    let mut last_idx = 0usize;
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));
    loop {
        interval.tick().await;
        let recent = state.events.recent(100);
        if recent.len() > last_idx {
            for ev in &recent[last_idx..] {
                let payload = serde_json::to_string(ev).unwrap_or_default();
                if socket.send(Message::Text(payload)).await.is_err() {
                    return;
                }
            }
            last_idx = recent.len();
        } else {
            // keepalive
            let ping = EntityEvent::Heartbeat { ts: Utc::now() };
            let payload = serde_json::to_string(&ping).unwrap_or_default();
            if socket.send(Message::Text(payload)).await.is_err() {
                return;
            }
        }
    }
}

/// Construit le routeur axum.
pub fn router(state: GatewayState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/ask", post(handle_ask))
        .route("/v1/goal", post(handle_create_goal))
        .route("/v1/plan/:goal_id", post(handle_plan))
        .route("/v1/execute/:goal_id", post(handle_execute_plan))
        .route("/v1/run", post(handle_shell))
        .route("/v1/cycle", post(handle_cycle))
        .route("/v1/status", get(handle_status))
        .route("/v1/goals", get(handle_list_goals))
        .route("/v1/events", get(handle_recent_events))
        .route("/v1/stream", get(handle_ws))
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(state)
}

/// Lance le serveur sur l'adresse donnée. Bloque jusqu'à arrêt.
pub async fn serve(state: GatewayState, addr: SocketAddr) -> std::io::Result<()> {
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("soul_gateway listening on {}", addr);
    axum::serve(listener, app).await
}
