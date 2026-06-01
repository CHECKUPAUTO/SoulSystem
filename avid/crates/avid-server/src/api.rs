use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use subtle::ConstantTimeEq;

use avid_core::config::Config;
use avid_core::llm::LlmClient;
use avid_core::memory::MemoryStore;

/// État partagé de l'API.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub memory: Arc<MemoryStore>,
    #[allow(dead_code)]
    pub llm: Arc<LlmClient>,
    pub headless: Option<std::sync::Arc<avid_scout::headless::HeadlessBrowser>>,
}

/// Requête de navigation headless.
#[derive(Debug, Deserialize)]
pub struct NavigateRequest {
    pub url: String,
    pub wait_for: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub screenshot: bool,
    #[serde(default = "default_viewport_w")]
    pub viewport_w: u32,
    #[serde(default = "default_viewport_h")]
    pub viewport_h: u32,
    #[serde(default)]
    pub fill: Vec<String>,
    #[serde(default)]
    pub click: Vec<String>,
}

const fn default_timeout() -> u64 {
    15000
}
const fn default_viewport_w() -> u32 {
    1920
}
const fn default_viewport_h() -> u32 {
    1080
}

/// Réponse de navigation.
#[derive(Debug, Serialize)]
pub struct NavigateResponse {
    pub url: String,
    pub html: Option<String>,
    pub screenshot: Option<String>, // base64 PNG
    pub screenshot_size: Option<usize>,
    pub timing_ms: u64,
}

/// Constructs the API router.
pub fn build_router(state: AppState) -> Router {
    let v1_routes = Router::new()
        .route("/scout", post(v1_not_implemented))
        .route("/vision", post(v1_not_implemented))
        .route("/cortex/paper", post(v1_not_implemented))
        .route("/mimic", post(v1_not_implemented))
        .route("/clone-pipeline", post(v1_not_implemented))
        .route("/headless/navigate", post(v1_headless_navigate))
        .layer(middleware::from_fn_with_state(state.clone(), auth));

    Router::new()
        .route("/healthz", get(healthz))
        .nest("/v1", v1_routes)
        .layer(tower_http::limit::RequestBodyLimitLayer::new(
            10 * 1024 * 1024,
        ))
        .with_state(state)
}

/// Bearer token authentication middleware.
/// Compares the provided token with `AVID_API_TOKEN` in constant time.
///
/// # Errors
/// Returns `StatusCode::UNAUTHORIZED` if the token is missing, malformed, or invalid.
pub async fn auth(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    if let Some(auth_header) = auth_header {
        if let Some(token) = auth_header.strip_prefix("Bearer ") {
            if token
                .as_bytes()
                .ct_eq(state.config.api_token.as_bytes())
                .into()
            {
                return Ok(next.run(req).await);
            }
        }
    }

    Err(StatusCode::UNAUTHORIZED)
}

async fn healthz() -> impl IntoResponse {
    Json(json!({"status": "ok"}))
}

async fn v1_not_implemented() -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({"error": "v1 endpoint not yet implemented"})),
    )
}

async fn v1_headless_navigate(
    State(state): State<AppState>,
    Json(req): Json<NavigateRequest>,
) -> impl IntoResponse {
    use std::time::Instant;

    let headless = match &state.headless {
        Some(h) => h.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(
                    json!({"error": "headless browser not initialized. Start with AVID_HEADLESS=1"}),
                ),
            );
        }
    };

    let start = Instant::now();

    let session = match headless.new_session(req.viewport_w, req.viewport_h).await {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("session creation failed: {e}")})),
            );
        }
    };

    // Navigate
    let html = match session.fetch_rendered(&req.url, req.timeout_ms).await {
        Ok(h) => Some(h),
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("navigation failed: {e}")})),
            );
        }
    };

    // Fill form fields
    for f in &req.fill {
        let parts: Vec<&str> = f.splitn(2, '=').collect();
        if parts.len() == 2 {
            let js = format!(
                "document.querySelector('{}').value = '{}';",
                parts[0].replace('\'', "\\'"),
                parts[1].replace('\'', "\\'")
            );
            let _ = session.eval(&js).await;
        }
    }

    // Click elements
    for sel in &req.click {
        let js = format!(
            "document.querySelector('{}').click();",
            sel.replace('\'', "\\'")
        );
        let _ = session.eval(&js).await;
    }

    // Wait for selector
    if let Some(ref sel) = req.wait_for {
        let _ = session.wait_for(sel, req.timeout_ms).await;
    }

    let timing_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);

    // Screenshot
    let (screenshot_b64, screenshot_size) = if req.screenshot {
        session
            .screenshot(true)
            .await
            .map_or((None, None), |bytes| {
                let size = bytes.len();
                (Some(base64_encode(&bytes)), Some(size))
            })
    } else {
        (None, None)
    };

    (
        StatusCode::OK,
        Json(
            serde_json::to_value(NavigateResponse {
                url: req.url,
                html,
                screenshot: screenshot_b64,
                screenshot_size,
                timing_ms,
            })
            .unwrap_or_default(),
        ),
    )
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
