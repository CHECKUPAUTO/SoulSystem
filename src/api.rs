//! API HTTP REST — Interface pour les agents OpenClaw.
//!
//! Expose les capacités de SoulSystem via HTTP.

use axum::{
    routing::{get, post},
    Json, Router,
    extract::{State, Path},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;

use crate::bound_system::BoundSystem;
use crate::pty_terminal::PtyTerminal;

/// État partagé de l'API.
pub struct ApiState {
    pub bound_system: Arc<BoundSystem>,
    pub pty_sessions: Arc<Mutex<HashMap<String, PtyTerminal>>>,
}

/// Requête d'exécution shell.
#[derive(Debug, Deserialize)]
pub struct ExecRequest {
    pub command: String,
    pub timeout_secs: Option<u64>,
}

/// Réponse d'exécution shell.
#[derive(Debug, Serialize)]
pub struct ExecResponse {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub success: bool,
}

/// Requête création PTY.
#[derive(Debug, Deserialize)]
pub struct PtyCreateRequest {
    pub session_id: Option<String>,
}

/// Réponse création PTY.
#[derive(Debug, Serialize)]
pub struct PtyCreateResponse {
    pub session_id: String,
    pub created: bool,
}

/// Requête écriture PTY.
#[derive(Debug, Deserialize)]
pub struct PtyWriteRequest {
    pub session_id: String,
    pub input: String,
}

/// Réponse lecture PTY.
#[derive(Debug, Serialize)]
pub struct PtyReadResponse {
    pub output: String,
    pub session_id: String,
}

/// Health check response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub version: String,
    pub features: Vec<String>,
}

/// Construit le router API.
pub fn router(state: Arc<ApiState>) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/api/exec", post(exec_handler))
        .route("/api/pty/create", post(pty_create_handler))
        .route("/api/pty/write", post(pty_write_handler))
        .route("/api/pty/read/:session_id", get(pty_read_handler))
        .route("/api/pty/destroy", post(pty_destroy_handler))
        .with_state(state)
}

async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        version: env!("CARGO_PKG_VERSION").to_string(),
        features: vec![
            "shell".to_string(),
            "pty".to_string(),
            "sandbox".to_string(),
        ],
    })
}

async fn exec_handler(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<ExecRequest>,
) -> Result<Json<ExecResponse>, StatusCode> {
    let timeout = req.timeout_secs.unwrap_or(60);
    
    match state.bound_system.execute(&req.command).await {
        Ok(result) => Ok(Json(ExecResponse {
            stdout: result.stdout,
            stderr: result.stderr,
            exit_code: result.exit_code,
            success: result.exit_code == 0,
        })),
        Err(e) => {
            tracing::error!("API exec error: {}", e);
            Ok(Json(ExecResponse {
                stdout: String::new(),
                stderr: format!("Error: {}", e),
                exit_code: -1,
                success: false,
            }))
        }
    }
}

async fn pty_create_handler(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<PtyCreateRequest>,
) -> Result<Json<PtyCreateResponse>, StatusCode> {
    let session_id = req.session_id.unwrap_or_else(|| {
        format!("pty_{}_{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis())
    });
    
    match PtyTerminal::new() {
        Ok(pty) => {
            let mut sessions = state.pty_sessions.lock().await;
            sessions.insert(session_id.clone(), pty);
            Ok(Json(PtyCreateResponse {
                session_id,
                created: true,
            }))
        }
        Err(e) => {
            tracing::error!("PTY create error: {}", e);
            Ok(Json(PtyCreateResponse {
                session_id,
                created: false,
            }))
        }
    }
}

async fn pty_write_handler(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<PtyWriteRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let sessions = state.pty_sessions.lock().await;
    
    if let Some(pty) = sessions.get(&req.session_id) {
        match pty.write(&req.input) {
            Ok(_) => Ok(Json(serde_json::json!({
                "session_id": req.session_id,
                "written": req.input.len(),
                "ok": true
            }))),
            Err(e) => Ok(Json(serde_json::json!({
                "session_id": req.session_id,
                "error": format!("{}", e),
                "ok": false
            })))
        }
    } else {
        Ok(Json(serde_json::json!({
            "session_id": req.session_id,
            "error": "Session not found",
            "ok": false
        })))
    }
}

async fn pty_read_handler(
    State(state): State<Arc<ApiState>>,
    Path(session_id): Path<String>,
) -> Result<Json<PtyReadResponse>, StatusCode> {
    let sessions = state.pty_sessions.lock().await;
    
    if let Some(pty) = sessions.get(&session_id) {
        match pty.read() {
            Ok(output) => Ok(Json(PtyReadResponse {
                output,
                session_id,
            })),
            Err(e) => Ok(Json(PtyReadResponse {
                output: format!("Error: {}", e),
                session_id,
            }))
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
    let mut sessions = state.pty_sessions.lock().await;
    sessions.remove(&req.session_id);
    
    Ok(Json(serde_json::json!({
        "session_id": req.session_id,
        "destroyed": true
    })))
}
