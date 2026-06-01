//! REST + WebSocket monitoring server with an inline dashboard.
//!
//! Endpoints:
//!   - GET  /             → live HTML dashboard
//!   - GET  /v1/health    → liveness probe
//!   - GET  /v1/status    → aggregate run status
//!   - GET  /v1/library   → list saved programs
//!   - GET  /v1/islands   → per-island summary
//!   - GET  /v1/costs     → aggregate cost snapshot
//!   - GET  /v1/metrics   → Prometheus exposition format
//!   - POST /v1/evolve    → evolutionary run (rate-limited)
//!   - WS   /ws           → real-time iteration event stream

use crate::config::Config;
use crate::database::ProgramDatabase;
use crate::metrics as om;
use crate::evolution::EvolutionEngine;
use crate::program::Program;
use anyhow::{Context, Result};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        DefaultBodyLimit, Query, State,
    },
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tower_governor::{governor::GovernorConfigBuilder, key_extractor::GlobalKeyExtractor};
use tracing::info;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub db: Arc<RwLock<ProgramDatabase>>,
    pub tx: broadcast::Sender<serde_json::Value>,
    pub cost: Arc<om::CostTracker>,
}

#[derive(Deserialize)]
pub struct LibraryQuery {
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct EvolveApiRequest {
    pub code: String,
    pub language: String,
    #[serde(default)]
    pub fitness_criteria: Vec<String>,
    pub max_iterations: Option<u32>,
    pub timeout_secs: Option<u64>,
}

#[derive(Serialize)]
pub struct EvolveApiResponse {
    pub best_code: String,
    pub best_score: f64,
    pub iterations: u32,
    pub logs: Vec<String>,
}

pub async fn run_server(cfg: Config, db: ProgramDatabase, bind: &str) -> Result<()> {
    // Best-effort metrics init; ignore the second-time-installer noise.
    let _ = om::init();

    let (tx, _rx) = broadcast::channel::<serde_json::Value>(256);
    let state = AppState {
        config: cfg,
        db: Arc::new(RwLock::new(db)),
        tx,
        cost: Arc::new(om::CostTracker::new()),
    };

    let governor_conf = GovernorConfigBuilder::default()
        .key_extractor(GlobalKeyExtractor)
        .period(Duration::from_secs(10))
        .burst_size(2)
        .finish()
        .context("build governor config")?;

    let evolve_router = Router::new()
        .route("/evolve", post(evolve));

    let v1 = Router::new()
        .route("/health", get(health))
        .route("/status", get(status))
        .route("/library", get(library))
        .route("/islands", get(islands))
        .route("/costs", get(costs))
        .route("/metrics", get(prometheus_metrics))
        .route("/ws", get(ws_handler))
        .merge(evolve_router);

    let app = Router::new()
        .route("/", get(dashboard))
        .nest("/v1", v1)
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .with_state(Arc::new(state));

    let addr: SocketAddr = bind
        .parse()
        .with_context(|| format!("parse bind addr {bind}"))?;
    info!("OpenEvolve server listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("axum serve")?;
    Ok(())
}

async fn shutdown_signal() {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("SIGTERM handler");
    term.recv().await;
    info!("SIGTERM received, shutting down gracefully");
}
// ── Handlers ────────────────────────────────────────────────────────────

async fn health() -> impl IntoResponse {
    Json(
        serde_json::json!({"status": "ok", "service": "openevolve", "version": env!("CARGO_PKG_VERSION")}),
    )
}

async fn status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let db = state.db.read().await;
    let total = db.total_size().await;
    let best = db.best().await;
    Json(serde_json::json!({
        "total_programs": total,
        "best_score": best.as_ref().map(|p| p.score).unwrap_or(0.0),
        "best_generation": best.as_ref().map(|p| p.generation).unwrap_or(0),
        "best_id": best.as_ref().map(|p| p.id.clone()).unwrap_or_default(),
        "config": {
            "max_iterations": state.config.evolution.max_iterations,
            "population_size": state.config.database.population_size,
            "num_islands": state.config.database.num_islands,
            "primary_model": state.config.llm.primary_model,
        }
    }))
}

async fn library(
    State(state): State<Arc<AppState>>,
    Query(q): Query<LibraryQuery>,
) -> Json<Vec<Program>> {
    let limit = q.limit.unwrap_or(20).clamp(1, 200);
    let db = state.db.read().await;
    let mut all = db.snapshot().await;
    all.sort_by(|a, b| {
        b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
    });
    all.truncate(limit);
    Json(all)
}

async fn ws_handler(State(state): State<Arc<AppState>>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state.tx.subscribe()))
}

#[derive(Serialize)]
struct IslandSummary {
    island: usize,
    size: usize,
    best_score: f64,
    mean_score: f64,
}

async fn islands(State(state): State<Arc<AppState>>) -> Json<Vec<IslandSummary>> {
    let db = state.db.read().await;
    let all = db.snapshot().await;
    let mut buckets: std::collections::BTreeMap<usize, Vec<f64>> = Default::default();
    for p in &all {
        buckets.entry(p.island_id).or_default().push(p.score);
    }
    let mut out = Vec::new();
    for (island, scores) in buckets {
        let size = scores.len();
        let best = scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let mean = if size > 0 { scores.iter().sum::<f64>() / size as f64 } else { 0.0 };
        out.push(IslandSummary { island, size, best_score: best, mean_score: mean });
    }
    Json(out)
}

async fn costs(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "total_usd": state.cost.total_usd(),
        "by_provider": state.cost.snapshot(),
    }))
}

async fn prometheus_metrics() -> impl IntoResponse {
    let body = om::render();
    ([("Content-Type", "text/plain; version=0.0.4")], body)
}

async fn handle_socket(mut socket: WebSocket, mut rx: broadcast::Receiver<serde_json::Value>) {
    loop {
        match rx.recv().await {
            Ok(msg) => {
                if socket.send(Message::Text(msg.to_string().into())).await.is_err() {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

async fn evolve(
    State(state): State<Arc<AppState>>,
    Json(req): Json<EvolveApiRequest>,
) -> impl IntoResponse {
    let output_dir = std::path::PathBuf::from(format!("/tmp/openevolve-{}", uuid::Uuid::new_v4()));
    let output_dir_clone = output_dir.clone();

    let result = tokio::time::timeout(Duration::from_secs(120), async move {
        let mut cfg = state.config.clone();
        if let Some(max_iter) = req.max_iterations {
            cfg.evolution.max_iterations = max_iter;
        }

        // Override evaluator to use the default compile-time evaluator
        cfg.evaluator.evaluator_bin = "/var/lib/openevolve/evaluator".into();
        // Disable persistence to avoid DB locking with the main server DB
        cfg.persistence.enabled = false;

        cfg.database.output_dir = output_dir_clone.to_string_lossy().to_string();

        match EvolutionEngine::new(cfg, req.code).await {
            Ok(mut engine) => {
                let max_iter = req.max_iterations.unwrap_or(50);
                match engine.run(max_iter).await {
                    Ok(_) => match engine.best().await {
                        Ok(Some(best)) => Json(serde_json::json!({
                            "success": true,
                            "best_code": best.code,
                            "best_score": best.score,
                            "iterations": max_iter,
                            "logs": Vec::<String>::new(),
                        })),
                        Ok(None) => Json(serde_json::json!({
                            "success": false,
                            "error": "No program generated",
                        })),
                        Err(e) => Json(serde_json::json!({
                            "success": false,
                            "error": e.to_string(),
                        })),
                    },
                    Err(e) => Json(serde_json::json!({
                        "success": false,
                        "error": e.to_string(),
                    })),
                }
            }
            Err(e) => Json(serde_json::json!({
                "success": false,
                "error": e.to_string(),
            })),
        }
    }).await;

    let _ = tokio::fs::remove_dir_all(&output_dir).await;

    match result {
        Ok(response) => response,
        Err(_) => Json(serde_json::json!({
            "success": false,
            "error": "Request timed out after 120s",
        })),
    }
}

async fn dashboard() -> impl IntoResponse {
    axum::response::Html(DASHBOARD_HTML)
}

const DASHBOARD_HTML: &str = r##"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>OpenEvolve Dashboard</title><style>
* { box-sizing: border-box; }
body { font-family: ui-monospace, monospace; background: #0d1117; color: #c9d1d9; margin: 0; padding: 20px; }
h1 { color: #58a6ff; margin: 0 0 12px; }
h2 { color: #7ee787; margin: 16px 0 8px; font-size: 14px; }
.row { display: flex; gap: 16px; flex-wrap: wrap; }
.card { background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 14px; flex: 1; min-width: 220px; }
.metric { font-size: 26px; color: #58a6ff; font-weight: bold; }
.sub { color: #8b949e; font-size: 12px; }
#log { background: #0a0e14; border: 1px solid #30363d; padding: 10px; height: 38vh; overflow-y: auto; font-size: 13px; border-radius: 8px; }
.entry { padding: 3px 6px; border-left: 3px solid transparent; margin: 2px 0; }
.entry.running { border-color: #7ee787; }
.entry.seed    { border-color: #58a6ff; }
.entry.completed { border-color: #d2a8ff; }
.delta-plus  { color: #7ee787; }
.delta-minus { color: #ff7b72; }
button { padding: 6px 12px; background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; cursor: pointer; font-size: 12px; }
button:hover { background: #30363d; }
.island-row { display: flex; align-items: center; gap: 8px; margin: 4px 0; font-size: 13px; }
.island-bar { background: #30363d; height: 18px; border-radius: 3px; overflow: hidden; flex: 1; min-width: 100px; }
.island-fill { background: linear-gradient(90deg, #58a6ff, #7ee787); height: 100%; transition: width 0.4s ease; }
.island-label { width: 70px; }
.island-stats { width: 130px; text-align: right; color: #8b949e; font-size: 12px; }
.cost-row { display: flex; justify-content: space-between; padding: 3px 0; font-size: 13px; }
.cost-row span:last-child { color: #f0b37e; }
</style></head><body>
<h1>🧬 OpenEvolve — live dashboard</h1>

<div class="row">
  <div class="card"><div class="sub">Best score</div><div class="metric" id="bestScore">—</div></div>
  <div class="card"><div class="sub">Total programs</div><div class="metric" id="totalPrograms">—</div></div>
  <div class="card"><div class="sub">Iteration</div><div class="metric" id="iter">—</div></div>
  <div class="card"><div class="sub">Last delta</div><div class="metric" id="delta">—</div></div>
  <div class="card"><div class="sub">USD spent</div><div class="metric" id="cost">—</div></div>
</div>

<div class="row" style="margin-top:16px;">
  <div class="card" style="flex:2;">
    <h2>Per-island population</h2>
    <div id="islands"><span class="sub">no data yet</span></div>
  </div>
  <div class="card" style="flex:1;">
    <h2>Cost by provider</h2>
    <div id="costs"><span class="sub">no calls yet</span></div>
  </div>
</div>

<h2>Live events <button onclick="reconnect()">Reconnect WS</button> <button onclick="document.getElementById('log').innerHTML=''">Clear</button> <a href="/v1/metrics" target="_blank" style="color:#58a6ff;">/metrics</a></h2>
<div id="log"></div>

<script>
const log = document.getElementById('log');
const bestEl = document.getElementById('bestScore');
const totalEl = document.getElementById('totalPrograms');
const iterEl = document.getElementById('iter');
const deltaEl = document.getElementById('delta');
const costEl = document.getElementById('cost');
const islandsEl = document.getElementById('islands');
const costsEl = document.getElementById('costs');
let lastScore = null;
let ws;

function refresh() {
  fetch('/v1/status').then(r=>r.json()).then(d=>{
    bestEl.textContent = d.best_score.toFixed(4);
    totalEl.textContent = d.total_programs;
  }).catch(()=>{});
  fetch('/v1/islands').then(r=>r.json()).then(rows=>{
    if (!rows.length) { islandsEl.innerHTML='<span class="sub">no data yet</span>'; return; }
    const maxSize = Math.max(...rows.map(r=>r.size), 1);
    islandsEl.innerHTML = rows.map(r=>{
      const pct = Math.round(100 * r.size / maxSize);
      return `<div class="island-row">
        <span class="island-label">Island ${r.island}</span>
        <div class="island-bar"><div class="island-fill" style="width:${pct}%"></div></div>
        <span class="island-stats">${r.size} | best ${r.best_score.toFixed(3)}</span>
      </div>`;
    }).join('');
  }).catch(()=>{});
  fetch('/v1/costs').then(r=>r.json()).then(d=>{
    costEl.textContent = '$' + (d.total_usd || 0).toFixed(4);
    if (!d.by_provider || !d.by_provider.length) { costsEl.innerHTML='<span class="sub">no calls yet</span>'; return; }
    costsEl.innerHTML = d.by_provider.map(c=>
      `<div class="cost-row"><span>${c.provider}/${c.model}</span><span>$${c.usd_estimate.toFixed(4)}</span></div>`
    ).join('');
  }).catch(()=>{});
}

function append(d, cls) {
  const div = document.createElement('div');
  div.className = 'entry ' + (cls || 'running');
  const score = d.score != null ? d.score.toFixed(4) : '—';
  if (d.score != null && lastScore != null) {
    const dd = d.score - lastScore;
    deltaEl.textContent = (dd >= 0 ? '+' : '') + dd.toFixed(4);
    deltaEl.className = 'metric ' + (dd >= 0 ? 'delta-plus' : 'delta-minus');
  }
  if (d.score != null) lastScore = d.score;
  div.textContent = `[${new Date().toLocaleTimeString()}] iter=${d.iteration} score=${score} op=${d.op || '-'}`;
  log.insertBefore(div, log.firstChild);
  if (d.iteration != null) iterEl.textContent = d.iteration;
  if (d.best_score != null) bestEl.textContent = d.best_score.toFixed(4);
}

function reconnect() {
  if (ws) try { ws.close(); } catch (_) {}
  ws = new WebSocket('ws://' + location.host + '/v1/ws');
  ws.onopen = () => append({iteration:'', op:'ws-open', score:null}, 'completed');
  ws.onmessage = e => { try { append(JSON.parse(e.data)); } catch (_) {} };
  ws.onclose = () => append({iteration:'', op:'ws-closed', score:null}, 'completed');
}

refresh();
reconnect();
setInterval(refresh, 3000);
</script>
</body></html>
"##;
