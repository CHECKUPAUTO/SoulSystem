//! API HTTP REST — Interface pour les agents OpenClaw.
//!
//! Expose les capacités de SoulSystem via HTTP, y compris les endpoints mémoire.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::bound_system::BoundSystem;
use crate::memory_hub::MemoryHub;
use crate::pty_terminal::PtyTerminal;

/// État partagé de l'API.
pub struct ApiState {
    pub bound_system: Arc<BoundSystem>,
    pub pty_sessions: Arc<Mutex<HashMap<String, PtyTerminal>>>,
    pub memory: Option<Arc<MemoryHub>>,
}

// ── Requêtes / Réponses existantes ─────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ExecRequest {
    pub command: String,
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct ExecResponse {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub success: bool,
}

#[derive(Debug, Deserialize)]
pub struct PtyCreateRequest {
    pub session_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PtyCreateResponse {
    pub session_id: String,
    pub created: bool,
}

#[derive(Debug, Deserialize)]
pub struct PtyWriteRequest {
    pub session_id: String,
    pub input: String,
}

#[derive(Debug, Serialize)]
pub struct PtyReadResponse {
    pub output: String,
    pub session_id: String,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub version: String,
    pub features: Vec<String>,
}

// ── Requêtes / Réponses Mémoire ────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct MemoryStoreRequest {
    pub text: String,
    pub metadata: Option<HashMap<String, String>>,
    pub tag: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MemoryStoreResponse {
    pub stored: bool,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MemorySearchRequest {
    pub query: String,
    pub top_k: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct MemorySearchResponse {
    pub results: Vec<MemoryHit>,
}

#[derive(Debug, Serialize)]
pub struct MemoryHit {
    pub text: String,
    pub score: f32,
    pub source: String,
}

#[derive(Debug, Deserialize)]
pub struct MemoryContextRequest {
    pub query: String,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct MemoryContextResponse {
    pub context: String,
    pub hit_count: usize,
}

// ── Router ─────────────────────────────────────────────────────────────

pub fn router(state: Arc<ApiState>) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/api/exec", post(exec_handler))
        .route("/api/pty/create", post(pty_create_handler))
        .route("/api/pty/write", post(pty_write_handler))
        .route("/api/pty/read/:session_id", get(pty_read_handler))
        .route("/api/pty/destroy", post(pty_destroy_handler))
        .route("/api/memory/store", post(memory_store_handler))
        .route("/api/memory/search", post(memory_search_handler))
        .route("/api/memory/context", post(memory_context_handler))
        .route("/api/zerobot/chat", post(zerobot_chat_handler))
        .route("/api/zerobot/health", get(zerobot_health_handler))
        .with_state(state)
}

// ── Handlers existants ─────────────────────────────────────────────────

async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        version: env!("CARGO_PKG_VERSION").to_string(),
        features: vec![
            "shell".into(),
            "pty".into(),
            "sandbox".into(),
            "memory".into(),
        ],
    })
}

async fn exec_handler(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<ExecRequest>,
) -> Result<Json<ExecResponse>, StatusCode> {
    match state.bound_system.execute(&req.command).await {
        Ok(result) => Ok(Json(ExecResponse {
            stdout: result.stdout,
            stderr: result.stderr,
            exit_code: result.exit_code,
            success: result.exit_code == 0,
        })),
        Err(e) => Ok(Json(ExecResponse {
            stdout: String::new(),
            stderr: format!("Error: {}", e),
            exit_code: -1,
            success: false,
        })),
    }
}

async fn pty_create_handler(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<PtyCreateRequest>,
) -> Result<Json<PtyCreateResponse>, StatusCode> {
    let session_id = req.session_id.unwrap_or_else(|| {
        format!(
            "pty_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        )
    });
    match PtyTerminal::new() {
        Ok(pty) => {
            state
                .pty_sessions
                .lock()
                .await
                .insert(session_id.clone(), pty);
            Ok(Json(PtyCreateResponse {
                session_id,
                created: true,
            }))
        }
        Err(_e) => Ok(Json(PtyCreateResponse {
            session_id,
            created: false,
        })),
    }
}

async fn pty_write_handler(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<PtyWriteRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let sessions = state.pty_sessions.lock().await;
    if let Some(pty) = sessions.get(&req.session_id) {
        match pty.write(&req.input) {
            Ok(_) => Ok(Json(
                serde_json::json!({"session_id": req.session_id, "ok": true}),
            )),
            Err(e) => Ok(Json(
                serde_json::json!({"ok": false, "error": format!("{}", e)}),
            )),
        }
    } else {
        Ok(Json(
            serde_json::json!({"ok": false, "error": "Session not found"}),
        ))
    }
}

async fn pty_read_handler(
    State(state): State<Arc<ApiState>>,
    Path(session_id): Path<String>,
) -> Result<Json<PtyReadResponse>, StatusCode> {
    let sessions = state.pty_sessions.lock().await;
    if let Some(pty) = sessions.get(&session_id) {
        match pty.read() {
            Ok(output) => Ok(Json(PtyReadResponse { output, session_id })),
            Err(e) => Ok(Json(PtyReadResponse {
                output: format!("Error: {}", e),
                session_id,
            })),
        }
    } else {
        Ok(Json(PtyReadResponse {
            output: String::new(),
            session_id,
        }))
    }
}

async fn pty_destroy_handler(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<PtyWriteRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state.pty_sessions.lock().await.remove(&req.session_id);
    Ok(Json(serde_json::json!({"destroyed": true})))
}

// ── Handlers Mémoire ───────────────────────────────────────────────────

async fn memory_store_handler(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<MemoryStoreRequest>,
) -> Result<Json<MemoryStoreResponse>, StatusCode> {
    let hub = match &state.memory {
        Some(h) => h.clone(),
        None => {
            return Ok(Json(MemoryStoreResponse {
                stored: false,
                error: Some("memory not available".into()),
            }))
        }
    };
    let mut meta = req.metadata.unwrap_or_default();
    if let Some(tag) = &req.tag {
        meta.insert("tag".into(), tag.clone());
    }
    match hub.store(&req.text, meta).await {
        Ok(_) => Ok(Json(MemoryStoreResponse {
            stored: true,
            error: None,
        })),
        Err(e) => Ok(Json(MemoryStoreResponse {
            stored: false,
            error: Some(format!("{}", e)),
        })),
    }
}

async fn memory_search_handler(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<MemorySearchRequest>,
) -> Result<Json<MemorySearchResponse>, StatusCode> {
    let hub = match &state.memory {
        Some(h) => h.clone(),
        None => return Ok(Json(MemorySearchResponse { results: vec![] })),
    };
    let top_k = req.top_k.unwrap_or(5);
    let raw = hub.search(&req.query, top_k).await;
    let results: Vec<MemoryHit> = raw
        .into_iter()
        .map(|r| MemoryHit {
            text: r.text,
            score: r.score,
            source: r.source.to_string(),
        })
        .collect();
    Ok(Json(MemorySearchResponse { results }))
}

async fn memory_context_handler(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<MemoryContextRequest>,
) -> Result<Json<MemoryContextResponse>, StatusCode> {
    let hub = match &state.memory {
        Some(h) => h.clone(),
        None => {
            return Ok(Json(MemoryContextResponse {
                context: String::new(),
                hit_count: 0,
            }))
        }
    };
    let limit = req.limit.unwrap_or(5);
    let context = hub.get_context(&req.query, limit).await;
    let hit_count = if context.is_empty() {
        0
    } else {
        context.matches("[").count()
    };
    Ok(Json(MemoryContextResponse { context, hit_count }))
}

// ── ZeroBot handlers (proxy vers le service zerobot-api :8000) ──

#[derive(Debug, Serialize, Deserialize)]
pub struct ZeroBotChatRequest {
    pub message: String,
    pub session_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ZeroBotChatResponse {
    pub response: String,
    pub session_id: String,
}

async fn zerobot_chat_handler(
    Json(req): Json<ZeroBotChatRequest>,
) -> Result<Json<ZeroBotChatResponse>, StatusCode> {
    let url = "http://zerobot-api:8000/chat";
    let client = reqwest::Client::new();
    match client
        .post(url)
        .json(&req)
        .timeout(std::time::Duration::from_secs(120))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => match resp.json::<ZeroBotChatResponse>().await {
            Ok(body) => Ok(Json(body)),
            Err(e) => {
                eprintln!("zerobot chat parse error: {}", e);
                Err(StatusCode::BAD_GATEWAY)
            }
        },
        Ok(resp) => {
            eprintln!(
                "zerobot chat HTTP {}: {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            );
            Err(StatusCode::BAD_GATEWAY)
        }
        Err(e) => {
            eprintln!(
                "zerobot chat network error: {} (is zerobot-api running ?)",
                e
            );
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}

async fn zerobot_health_handler() -> Result<Json<serde_json::Value>, StatusCode> {
    let url = "http://zerobot-api:8000/health";
    let client = reqwest::Client::new();
    match client
        .get(url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => match resp.json::<serde_json::Value>().await {
            Ok(body) => Ok(Json(body)),
            Err(_) => Ok(Json(
                serde_json::json!({"zerobot": "reachable but parse failed"}),
            )),
        },
        Ok(_) => {
            eprintln!("zerobot health check failed");
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
        Err(e) => {
            eprintln!("zerobot health check network error: {}", e);
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}
