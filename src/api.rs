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
    pub metrics: crate::metrics::MetricsRegistry,
    pub bridge_store: Option<Arc<crate::bridge_store::BridgeStore>>,
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
        .route("/api/pty/read/{session_id}", get(pty_read_handler))
        .route("/api/pty/destroy", post(pty_destroy_handler))
        .route("/api/memory/store", post(memory_store_handler))
        .route("/api/memory/search", post(memory_search_handler))
        .route("/api/memory/context", post(memory_context_handler))
        .route("/api/zerobot/chat", post(zerobot_chat_handler))
        .route("/api/zerobot/health", get(zerobot_health_handler))
        // Bridges inter-composants : statut aller-retour
        .route("/api/bridges/status", get(bridges_status_handler))
        .route("/api/bridges/probe", post(bridges_probe_handler))
        .route("/api/bridges/organs", get(organs_status_handler))
        .route("/api/bridges/mesh", get(mesh_status_handler))
        .route("/api/bridges/services", get(services_status_handler))
        .route("/metrics", get(metrics_handler))
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

// ── Bridges status : expose l'état runtime de chaque bridge ────────────

#[derive(Debug, Serialize)]
struct BridgesStatus {
    avid: String,
    openevolve: String,
    synergie: String,
    brain: String,
    soul_neural: String,
}

async fn bridges_status_handler() -> Json<BridgesStatus> {
    let avid = probe_url("http://127.0.0.1:7878", "/health").await;
    let openevolve = probe_url("http://127.0.0.1:7879", "/v1/health").await;
    let synergie = probe_url("http://127.0.0.1:7460", "/health").await;
    let brain = probe_url("http://127.0.0.1:9010", "/api/health").await;
    let soul_neural = probe_url("http://127.0.0.1:9020", "/api/mesh/status").await;
    Json(BridgesStatus {
        avid,
        openevolve,
        synergie,
        brain,
        soul_neural,
    })
}

async fn bridges_probe_handler() -> Json<serde_json::Value> {
    let tasks = vec![
        ("avid", "http://127.0.0.1:7878", "/health"),
        ("openevolve", "http://127.0.0.1:7879", "/v1/health"),
        ("openevolve_status", "http://127.0.0.1:7879", "/v1/status"),
        ("synergie", "http://127.0.0.1:7460", "/health"),
        ("synergie_eco", "http://127.0.0.1:7460", "/ecosystem"),
        ("brain_science", "http://127.0.0.1:9010", "/api/health"),
        ("brain_mind", "http://127.0.0.1:9011", "/api/health"),
        ("brain_engineer", "http://127.0.0.1:9012", "/api/health"),
        ("brain_crypto", "http://127.0.0.1:9013", "/api/health"),
        ("brain_creative", "http://127.0.0.1:9014", "/api/health"),
        ("brain_meta", "http://127.0.0.1:9015", "/api/health"),
        ("orchestrator", "http://127.0.0.1:9020", "/api/mesh/status"),
        ("memory", "http://127.0.0.1:9030", "/api/stats"),
    ];
    let mut results = serde_json::Map::new();
    for (name, base, path) in tasks {
        let r = probe_url(base, path).await;
        results.insert(name.to_string(), serde_json::Value::String(r));
    }
    Json(serde_json::Value::Object(results))
}

async fn probe_url(base: &str, path: &str) -> String {
    let url = format!("{}{}", base, path);
    match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
    {
        Ok(client) => match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let body = resp.text().await.unwrap_or_default();
                let preview = if body.len() > 120 {
                    format!("{}…", &body[..120])
                } else {
                    body
                };
                format!("OK: {}", preview)
            }
            Ok(resp) => format!("HTTP {}", resp.status()),
            Err(e) => format!("ERR: {}", e),
        },
        Err(e) => format!("CLIENT ERR: {}", e),
    }
}

// ── Organs status : 12 organes HNN (V1 + V2) ───────────────────────

async fn organs_status_handler() -> Json<serde_json::Value> {
    // V1 organs (5031-5036) — /api/stats
    // V2 organs (9040-9046) — /api/stats
    let v1 = [
        ("reasoning", 5031),
        ("integration", 5032),
        ("perception", 5033),
        ("affect", 5034),
        ("reflex", 5035),
        ("language", 5036),
    ];
    let v2 = [
        ("foresight", 9040),
        ("homeostasis", 9041),
        ("creativity", 9042),
        ("social", 9043),
        ("validation", 9044),
        ("autonomy", 9046),
    ];

    let mut out = serde_json::Map::new();
    for (name, port) in v1.iter().chain(v2.iter()) {
        let r = probe_url(&format!("http://127.0.0.1:{}", port), "/").await;
        out.insert(
            name.to_string(),
            serde_json::json!({
                "port": port,
                "status": r,
            }),
        );
    }
    Json(serde_json::json!({
        "v1_count": v1.len(),
        "v2_count": v2.len(),
        "organs": out,
    }))
}

async fn mesh_status_handler() -> Json<serde_json::Value> {
    let services: Vec<(&str, u16, &str)> = vec![
        ("rag_turbo", 9070, "/health"),
        ("rowboat", 9071, "/health"),
        ("moe", 9072, "/health"),
        ("pacemaker", 9073, "/api/status"),
        ("blackboard", 9074, "/api/status"),
        ("memori", 9075, "/api/status"),
        ("v14", 9095, "/"),
        ("voice", 9050, "/"),
    ];
    let mut out = serde_json::Map::new();
    for (name, port, path) in services.iter() {
        let url = format!("http://127.0.0.1:{}", port);
        let r = probe_url(&url, path).await;
        out.insert(
            name.to_string(),
            serde_json::json!({
                "port": port,
                "path": path,
                "status": r,
            }),
        );
    }
    Json(serde_json::json!({
        "count": services.len(),
        "services": out,
    }))
}

// ── Services status : 12 services tiers (omniclaw, ollama, etc.) ─────────

async fn services_status_handler() -> Json<serde_json::Value> {
    let services: Vec<(&str, u16, &str)> = vec![
        ("omniclaw", 9091, "/api/health"),
        ("onaeu", 7878, "/health"),
        ("ollama", 11434, "/api/version"),
        ("turboquant-proxy", 11435, "/api/version"),
        ("soulbridge", 11436, "/health"),
        ("nats", 4222, "/varz"),
        ("crowdsec", 8083, "/v1/usage"),
        ("mirofish-router", 7470, "/health"),
        ("super-tool", 8085, "/health"),
        ("qmd-mcp", 8181, "/"),
        ("sl13-monolith", 9045, "/health"),
        ("novnc", 9060, "/"),
    ];
    let mut out = serde_json::Map::new();
    for (name, port, path) in services.iter() {
        let url = format!("http://127.0.0.1:{}", port);
        let r = probe_url(&url, path).await;
        out.insert(
            name.to_string(),
            serde_json::json!({
                "port": port,
                "path": path,
                "status": r,
            }),
        );
    }
    Json(serde_json::json!({
        "count": services.len(),
        "services": out,
    }))
}

// ── Prometheus metrics endpoint ────────────────────────────────────

async fn metrics_handler(
    axum::extract::State(state): axum::extract::State<Arc<ApiState>>,
) -> impl axum::response::IntoResponse {
    let body = state.metrics.render().await;
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        body,
    )
}
