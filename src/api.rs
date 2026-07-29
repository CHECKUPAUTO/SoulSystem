//! API HTTP REST — Interface pour les agents OpenClaw.
//!
//! Expose les capacités de SoulSystem via HTTP, y compris les endpoints mémoire.

use axum::{
    extract::{Path, Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::Response,
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

/// Environment variable holding this listener's bearer token.
///
/// Separate from `SOULSYSTEM_GATEWAY_TOKEN` on purpose: these are different
/// listeners with different audiences, and a single shared value would mean
/// that rotating one forces rotating the other.
pub const API_TOKEN_VAR: &str = "SOULSYSTEM_API_TOKEN";

/// État partagé de l'API.
pub struct ApiState {
    pub bound_system: Arc<BoundSystem>,
    pub pty_sessions: Arc<Mutex<HashMap<String, PtyTerminal>>>,
    pub memory: Option<Arc<MemoryHub>>,
    pub metrics: crate::metrics::MetricsRegistry,
    pub bridge_store: Option<Arc<crate::bridge_store::BridgeStore>>,
    /// Bearer authentication for every route except `/health` (CRIT-007 /
    /// INV-NET-1). Fails closed: with no usable token configured, every
    /// request is rejected — there is no implicit "open" state.
    pub auth: ApiAuth,
}

/// Fail-closed bearer authenticator for the `api` listener.
///
/// This listener exposes `/api/exec` (shell execution via `BoundSystem`) and
/// `/api/pty/*` (interactive terminals), and had **no authentication of any
/// kind** — it was mitigated only by its `127.0.0.1:9023` bind, which does not
/// protect against anything already running as another user on the host, or
/// against a browser-driven request from a page the operator loaded.
#[derive(Clone, Default)]
pub struct ApiAuth {
    token: Option<Arc<str>>,
}

impl std::fmt::Debug for ApiAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the token itself (INV-ENV-3).
        f.debug_struct("ApiAuth")
            .field("configured", &self.is_configured())
            .finish()
    }
}

impl ApiAuth {
    /// Read the token from [`API_TOKEN_VAR`].
    ///
    /// A blank or whitespace-only value is treated as unset, so an empty
    /// config entry cannot become a credential.
    pub fn from_env() -> Self {
        Self::new(std::env::var(API_TOKEN_VAR).ok())
    }

    /// Build with an explicit token (tests, embedders).
    pub fn new(token: Option<String>) -> Self {
        let token = token
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty())
            .map(Arc::from);
        Self { token }
    }

    /// Whether a usable token is configured.
    pub fn is_configured(&self) -> bool {
        self.token.is_some()
    }

    /// Whether `provided` is the configured token.
    ///
    /// Returns `false` when nothing is configured — the fail-closed direction.
    /// The comparison is constant-time so response latency cannot be used to
    /// recover the token byte by byte.
    pub fn authenticate(&self, provided: Option<&str>) -> bool {
        let (Some(expected), Some(provided)) = (self.token.as_deref(), provided) else {
            return false;
        };
        constant_time_eq(expected.as_bytes(), provided.as_bytes())
    }
}

/// Compare without an early return on the first differing byte.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Reject any request without the configured bearer token.
async fn require_auth(
    State(state): State<Arc<ApiState>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let provided = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    if state.auth.authenticate(provided) {
        Ok(next.run(req).await)
    } else {
        Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "unauthorized".into(),
            }),
        ))
    }
}

/// One opaque rejection body, so a caller cannot distinguish "no token
/// configured" from "wrong token".
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
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

/// Construit le routeur axum.
///
/// Every route except `/health` requires a valid `Authorization: Bearer
/// <token>` header, enforced by [`require_auth`] (CRIT-007 / INV-NET-1).
/// `/health` is the sole exception: it is a liveness probe and carries no
/// state beyond the crate version.
///
/// `/metrics` is **inside** the authenticated set. It is a disclosure route —
/// request counts and error rates describe what the host is doing — and the
/// same reasoning that put the gateway's `/v1/status` behind auth applies.
/// A scraper therefore needs the token.
pub fn router(state: Arc<ApiState>) -> Router {
    let authenticated = Router::new()
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
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .route("/health", get(health_handler))
        .merge(authenticated)
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
                .unwrap_or_else(|_| std::time::Duration::from_secs(0))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn state(auth: ApiAuth) -> Arc<ApiState> {
        Arc::new(ApiState {
            bound_system: Arc::new(BoundSystem::new(BoundSystem::default_whitelist())),
            pty_sessions: Arc::new(Mutex::new(HashMap::new())),
            memory: None,
            metrics: crate::metrics::MetricsRegistry::default(),
            bridge_store: None,
            auth,
        })
    }

    async fn send(app: Router, method: &str, uri: &str, bearer: Option<&str>) -> StatusCode {
        use tower::ServiceExt;
        let mut builder = axum::http::Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json");
        if let Some(token) = bearer {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let req = builder.body(axum::body::Body::from("{}")).unwrap();
        app.oneshot(req).await.unwrap().status()
    }

    // ── ApiAuth ──────────────────────────────────────────────────────────

    #[test]
    fn auth_is_unconfigured_by_default_and_rejects_everything() {
        let auth = ApiAuth::default();
        assert!(!auth.is_configured());
        assert!(!auth.authenticate(Some("anything")));
        assert!(!auth.authenticate(None));
    }

    #[test]
    fn a_blank_token_is_treated_as_unset_rather_than_as_a_credential() {
        for blank in ["", "   ", "\t\n"] {
            let auth = ApiAuth::new(Some(blank.into()));
            assert!(!auth.is_configured(), "{blank:?} must not configure auth");
            assert!(!auth.authenticate(Some(blank)));
        }
    }

    #[test]
    fn auth_accepts_only_the_exact_token() {
        let auth = ApiAuth::new(Some("  s3cret  ".into()));
        assert!(auth.is_configured());
        assert!(
            auth.authenticate(Some("s3cret")),
            "surrounding space is trimmed"
        );
        assert!(!auth.authenticate(Some("s3cre")));
        assert!(!auth.authenticate(Some("s3crett")));
        assert!(!auth.authenticate(Some("S3CRET")));
        assert!(!auth.authenticate(None));
    }

    #[test]
    fn debug_output_never_reveals_the_token() {
        let rendered = format!("{:?}", ApiAuth::new(Some("super-secret-value".into())));
        assert!(
            !rendered.contains("super-secret-value"),
            "INV-ENV-3: {rendered}"
        );
        assert!(rendered.contains("configured"));
    }

    #[test]
    fn constant_time_eq_rejects_length_mismatch() {
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"", b"x"));
        assert!(constant_time_eq(b"", b""));
    }

    // ── Router / CRIT-007: the api listener is no longer open ────────────

    #[tokio::test]
    async fn health_is_reachable_without_auth() {
        let app = router(state(ApiAuth::new(Some("tok".into()))));
        assert_eq!(send(app, "GET", "/health", None).await, StatusCode::OK);
    }

    #[tokio::test]
    async fn shell_execution_requires_auth() {
        // The route that made this listener worth closing: it runs commands.
        let app = router(state(ApiAuth::new(Some("tok".into()))));
        assert_eq!(
            send(app, "POST", "/api/exec", None).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn every_pty_route_requires_auth() {
        let app = router(state(ApiAuth::new(Some("tok".into()))));
        for (method, uri) in [
            ("POST", "/api/pty/create"),
            ("POST", "/api/pty/write"),
            ("GET", "/api/pty/read/abc"),
            ("POST", "/api/pty/destroy"),
        ] {
            assert_eq!(
                send(app.clone(), method, uri, None).await,
                StatusCode::UNAUTHORIZED,
                "{method} {uri} must be authenticated"
            );
        }
    }

    #[tokio::test]
    async fn disclosure_routes_require_auth_too_not_just_state_changing_ones() {
        let app = router(state(ApiAuth::new(Some("tok".into()))));
        for uri in [
            "/api/bridges/status",
            "/api/bridges/organs",
            "/api/bridges/mesh",
            "/api/bridges/services",
            "/metrics",
        ] {
            assert_eq!(
                send(app.clone(), "GET", uri, None).await,
                StatusCode::UNAUTHORIZED,
                "{uri} discloses host state and must be authenticated"
            );
        }
    }

    #[tokio::test]
    async fn a_wrong_token_is_rejected() {
        let app = router(state(ApiAuth::new(Some("tok".into()))));
        assert_eq!(
            send(app, "POST", "/api/exec", Some("nope")).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn the_correct_token_reaches_the_handler() {
        let app = router(state(ApiAuth::new(Some("tok".into()))));
        let status = send(app, "GET", "/api/bridges/status", Some("tok")).await;
        assert_ne!(
            status,
            StatusCode::UNAUTHORIZED,
            "a valid token must get past the middleware"
        );
    }

    #[tokio::test]
    async fn an_unconfigured_listener_rejects_every_request() {
        // Fail closed: with no token configured there is no implicit "open"
        // state, so even a request supplying SOME bearer value is refused.
        let app = router(state(ApiAuth::default()));
        assert_eq!(
            send(app.clone(), "POST", "/api/exec", Some("anything")).await,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(app.clone(), "POST", "/api/exec", None).await,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(app, "GET", "/health", None).await,
            StatusCode::OK,
            "the liveness probe stays reachable so an unconfigured deployment \
             is still diagnosable"
        );
    }
}
