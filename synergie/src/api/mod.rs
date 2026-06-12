//! HTTP/WS API — Axum 0.7.
//!
//! Endpoints :
//!   GET  /health
//!   GET  /ecosystem
//!   GET  /synergies            (?status=&type=&min_score=&limit=)
//!   GET  /synergy/:id
//!   POST /scan                 (déclenche un scan async)
//!   POST /feedback/:id         (body: {status, note?})
//!   GET  /report/latest
//!   GET  /proposals
//!   GET  /metrics
//!   GET  /ws                   (broadcast Event)
//!   POST /sync/github          (force push)
//!   GET  /status

use crate::agent::{Agent, Event};
use crate::config::Config;
use crate::types::SynergyStatus;
use anyhow::Result;
use axum::{
    extract::{Path, Query, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, warn};

#[derive(Clone)]
struct AppState {
    cfg: Arc<Config>,
    agent: Arc<Agent>,
}

pub async fn serve(cfg: Arc<Config>, agent: Arc<Agent>) -> Result<()> {
    let bind = format!("{}:{}", cfg.api.bind, cfg.api.port);
    let state = AppState {
        cfg: cfg.clone(),
        agent,
    };

    let mut router = Router::new()
        .route("/health", get(health))
        .route("/ecosystem", get(ecosystem))
        .route("/synergies", get(synergies))
        .route("/synergy/:id", get(synergy_by_id))
        .route("/scan", post(scan_trigger))
        .route("/feedback/:id", post(feedback))
        .route("/report/latest", get(report_latest))
        .route("/proposals", get(proposals))
        .route("/metrics", get(metrics))
        .route("/ws", get(ws_handler))
        .route("/sync/github", post(sync_github))
        .route("/status", get(status))
        .with_state(state);

    if cfg.api.cors_allow_any {
        router = router.layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );
    }

    info!(target: "api", bind = %bind, "HTTP/WS API démarrée");
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    axum::serve(listener, router).await?;
    Ok(())
}

// ─── Auth helper ────────────────────────────────────────────────────────

fn check_auth(headers: &HeaderMap, expected: &Option<String>) -> Result<(), StatusCode> {
    let Some(token) = expected else {
        return Ok(());
    };
    let h = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if h == format!("Bearer {}", token) || h == *token {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

// ─── Endpoints ──────────────────────────────────────────────────────────

#[derive(Serialize)]
struct HealthResp {
    status: &'static str,
    version: &'static str,
    counts: crate::memory::MemoryCounts,
}

async fn health(State(_s): State<AppState>) -> impl IntoResponse {
    let counts = crate::memory::counts().unwrap_or(crate::memory::MemoryCounts {
        synergies: 0,
        embed_cache: 0,
        history: 0,
        feedback: 0,
    });
    Json(HealthResp {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        counts,
    })
}

async fn ecosystem(State(s): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(c) = check_auth(&headers, &s.cfg.api.auth_token) {
        return (c, Json(serde_json::json!({})));
    }
    match crate::ecosystem::Ecosystem::discover(&s.cfg) {
        Ok(eco) => (
            StatusCode::OK,
            Json(serde_json::json!({"projects": eco.projects})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

#[derive(Deserialize)]
struct SynergyFilter {
    #[serde(default)]
    status: Option<String>,
    #[serde(default, rename = "type")]
    ty: Option<String>,
    #[serde(default)]
    min_score: Option<f32>,
    #[serde(default = "default_limit")]
    limit: usize,
}
fn default_limit() -> usize {
    100
}

async fn synergies(
    State(s): State<AppState>,
    Query(q): Query<SynergyFilter>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(c) = check_auth(&headers, &s.cfg.api.auth_token) {
        return (c, Json(serde_json::json!([])));
    }
    let mut list = crate::memory::list_synergies().unwrap_or_default();
    if let Some(st) = &q.status {
        list.retain(|x| format!("{:?}", x.status).eq_ignore_ascii_case(st));
    }
    if let Some(t) = &q.ty {
        list.retain(|x| x.synergy_type.eq_ignore_ascii_case(t));
    }
    if let Some(min) = q.min_score {
        list.retain(|x| x.adjusted_score >= min);
    }
    list.sort_by(|a, b| {
        b.adjusted_score
            .partial_cmp(&a.adjusted_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    list.truncate(q.limit);
    (
        StatusCode::OK,
        Json(serde_json::to_value(list).unwrap_or(serde_json::json!([]))),
    )
}

async fn synergy_by_id(
    State(s): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(c) = check_auth(&headers, &s.cfg.api.auth_token) {
        return (c, Json(serde_json::json!({})));
    }
    match crate::memory::get_synergy(&id) {
        Ok(Some(s)) => (
            StatusCode::OK,
            Json(serde_json::to_value(s).unwrap_or_default()),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "not found"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn scan_trigger(State(s): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(c) = check_auth(&headers, &s.cfg.api.auth_token) {
        return (c, Json(serde_json::json!({})));
    }
    let agent = s.agent.clone();
    tokio::spawn(async move {
        match agent.one_shot_scan(None).await {
            Ok(r) => {
                let _ = agent.write_report(&r).await;
                info!(target: "api", n = r.synergies.len(), "scan async terminé");
            }
            Err(e) => warn!(target: "api", error = %e, "scan async échoué"),
        }
    });
    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({"status": "started"})),
    )
}

#[derive(Deserialize)]
struct FeedbackBody {
    status: String,
    #[serde(default)]
    note: Option<String>,
}

async fn feedback(
    State(s): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<FeedbackBody>,
) -> impl IntoResponse {
    if let Err(c) = check_auth(&headers, &s.cfg.api.auth_token) {
        return (c, Json(serde_json::json!({})));
    }
    let status = match body.status.to_lowercase().as_str() {
        "implemented" => SynergyStatus::Implemented,
        "rejected" => SynergyStatus::Rejected,
        "investigating" => SynergyStatus::Investigating,
        "inprogress" | "in_progress" => SynergyStatus::InProgress,
        "stale" => SynergyStatus::Stale,
        _ => SynergyStatus::Proposed,
    };
    match crate::memory::record_feedback(&id, status, body.note) {
        Ok(syn) => {
            let _ = s.agent.events_tx().send(Event::SynergyUpdated {
                id: syn.id.clone(),
                status: format!("{:?}", syn.status),
            });
            (
                StatusCode::OK,
                Json(serde_json::to_value(syn).unwrap_or_default()),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn report_latest(State(s): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(c) = check_auth(&headers, &s.cfg.api.auth_token) {
        return (c, Json(serde_json::json!({})));
    }
    match s.agent.load_last_report().await {
        Ok(r) => (
            StatusCode::OK,
            Json(serde_json::to_value(r).unwrap_or_default()),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn proposals(State(s): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(c) = check_auth(&headers, &s.cfg.api.auth_token) {
        return (c, Json(serde_json::json!([])));
    }
    let mut out = Vec::new();
    if let Some(repo) = &s.cfg.action.github_proposals_repo {
        let dir = repo.join("proposals");
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    out.push(serde_json::json!({
                        "id": p.file_name().and_then(|s| s.to_str()).unwrap_or(""),
                        "path": p.to_string_lossy(),
                    }));
                }
            }
        }
    }
    (StatusCode::OK, Json(serde_json::json!(out)))
}

async fn metrics(State(_s): State<AppState>) -> impl IntoResponse {
    let counts = crate::memory::counts().unwrap_or(crate::memory::MemoryCounts {
        synergies: 0,
        embed_cache: 0,
        history: 0,
        feedback: 0,
    });
    let opt = crate::memory::load_optimizer().ok();
    let coefs: HashMap<String, f64> = opt
        .as_ref()
        .map(|o| o.coefficients().clone())
        .unwrap_or_default();
    Json(serde_json::json!({
        "synergies": counts.synergies,
        "embed_cache": counts.embed_cache,
        "history": counts.history,
        "feedback": counts.feedback,
        "rstdp_coefficients": coefs,
        "rstdp_lr": opt.as_ref().map(|o| o.lr()).unwrap_or(0.1),
        "rstdp_momentum": opt.as_ref().map(|o| o.momentum()).unwrap_or(0.9),
    }))
}

async fn ws_handler(ws: WebSocketUpgrade, State(s): State<AppState>) -> impl IntoResponse {
    let mut rx = s.agent.events_tx().subscribe();
    ws.on_upgrade(move |socket| async move {
        let (mut sink, mut stream) = socket.split();
        let hello = serde_json::json!({"type": "hello", "version": env!("CARGO_PKG_VERSION")});
        let _ = sink.send(axum::extract::ws::Message::Text(hello.to_string().into())).await;
        loop {
            tokio::select! {
                ev = rx.recv() => {
                    match ev {
                        Ok(ev) => {
                            let payload = serde_json::to_string(&ev).unwrap_or_default();
                            if sink.send(axum::extract::ws::Message::Text(payload.into())).await.is_err() { break; }
                        }
                        Err(_) => break,
                    }
                }
                msg = stream.next() => {
                    match msg {
                        Some(Ok(_)) => continue,
                        _ => break,
                    }
                }
            }
        }
    })
}

async fn sync_github(State(s): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(c) = check_auth(&headers, &s.cfg.api.auth_token) {
        return (c, Json(serde_json::json!({})));
    }
    let Some(repo) = &s.cfg.action.github_proposals_repo else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "no proposals repo configured"})),
        );
    };
    let remote = s
        .cfg
        .action
        .github_remote
        .clone()
        .unwrap_or_else(|| "origin".into());
    let branch = "main";
    match crate::github::push(repo, &remote, branch) {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "pushed"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

async fn status(State(s): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "tick_seconds": s.cfg.agent.tick_seconds,
        "action_threshold": s.cfg.agent.action_threshold,
        "dry_run": s.cfg.agent.dry_run,
        "detectors": s.cfg.detectors,
    }))
}
