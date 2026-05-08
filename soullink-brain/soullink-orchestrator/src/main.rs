//! SoulLink Orchestrator V13.5 — Rust Native
//!
//! Port of the legacy `orchestrator_v3` to the workspace, with Phase 1
//! security changes:
//!
//! * **bind on `127.0.0.1` by default** (legacy was `0.0.0.0`)
//! * **Bearer token auth** via `soullink-core::auth`, in cohabitation mode
//!   (localhost-no-token still allowed during Phase 1 for the Python bot)
//! * **domain regex** on `/api/mesh/spawn` — rejects shell-unsafe inputs
//! * **axum 0.6 → 0.7** migration (new `axum::serve` API)
//! * **minreq + spawn_blocking → reqwest async** (no blocked workers,
//!   keep-alive pool; Phase 2 will hoist the client to a shared `OnceLock`)
//!
//! Architecture unchanged: DashMap registry, Guardian snapshot ingestion,
//! turbulence-aware query routing, parallel brain fan-out.

mod config;
mod ipc_client;
mod mesh_context;
mod ollama_bridge;
mod routing;
mod synth_fallback;

use routing::{attractor_score, merge_concepts, select_brains as select_brains_pure};

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    extract::State,
    middleware,
    response::{
        sse::{Event, KeepAlive, Sse},
        Json,
    },
    routing::{get, post},
    Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::process::Command;
use tracing::{error, info, warn};

use soullink_core::{
    auth::{require_auth, AuthConfig},
    secrets, validation,
};

// ─── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BrainConfig {
    port:       u16,
    url:        String,
    speciality: Vec<String>,
    version:    String,
    #[serde(default)]
    spawned_by: String,
}

// ─── AppState ───────────────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    brains:       Arc<DashMap<String, BrainConfig>>,
    query_count:  Arc<AtomicU64>,
    spawn_count:  Arc<AtomicU64>,
    brain_dir:    String,
    guardian:     Arc<DashMap<String, Value>>,
    http:         reqwest::Client,
    /// Optional Ollama bridge config. `None` → fallback to symbolic synth.
    ollama:       Option<Arc<ollama_bridge::OllamaConfig>>,
}

impl AppState {
    fn new(brain_dir: &str) -> Self {
        let specs = [
            ("science",  9010u16, vec!["physics","math","chemistry","computation","science"]),
            ("mind",     9011,    vec!["neuroscience","language","philosophy","memory","mind"]),
            ("engineer", 9012,    vec!["optimization","logic","algebra","computation","engineering"]),
            ("crypto",   9013,    vec!["trading","blockchain","defi","markets","crypto","finance"]),
            ("creative", 9014,    vec!["patterns","geometry","art","vision","design","creative"]),
            ("meta",     9015,    vec!["learning","optimization","meta","reinforcement"]),
        ];

        let brains: DashMap<String, BrainConfig> = DashMap::new();
        for (name, port, spec) in &specs {
            brains.insert(
                name.to_string(),
                BrainConfig {
                    port:       *port,
                    url:        format!("http://127.0.0.1:{}", port),
                    speciality: spec.iter().map(|s| s.to_string()).collect(),
                    version:    "v12".to_string(),
                    spawned_by: "default".to_string(),
                },
            );
        }

        // Shared keep-alive HTTP client (one per process).
        // Phase 2 will promote this to a OnceLock + HTTP/2 prior knowledge.
        let http = reqwest::Client::builder()
            .pool_max_idle_per_host(32)
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_keepalive(Duration::from_secs(60))
            .timeout(Duration::from_secs(4))
            .build()
            .expect("reqwest client");

        Self {
            brains:      Arc::new(brains),
            query_count: Arc::new(AtomicU64::new(0)),
            spawn_count: Arc::new(AtomicU64::new(0)),
            brain_dir:   brain_dir.to_string(),
            guardian:    Arc::new(DashMap::new()),
            http,
            ollama:      None,
        }
    }
}

// ─── Brain calls ────────────────────────────────────────────────────────────

async fn call_brain(
    http: &reqwest::Client,
    url: &str,
    endpoint: &str,
    body: Option<Value>,
) -> Result<Value, String> {
    let full_url = format!("{}{}", url, endpoint);
    let resp = if let Some(b) = body {
        http.post(&full_url).json(&b).send().await
    } else {
        http.get(&full_url).send().await
    };

    match resp {
        Ok(r) => r.json::<Value>().await.map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

async fn call_all_parallel(
    state: &AppState,
    endpoint: &str,
    body: Option<Value>,
    keys: Option<Vec<String>>,
) -> HashMap<String, Value> {
    let keys = keys.unwrap_or_else(|| {
        state.brains.iter().map(|e| e.key().clone()).collect()
    });

    let futures: Vec<_> = keys
        .iter()
        .map(|k| {
            let url = state.brains.get(k).map(|b| b.url.clone()).unwrap_or_default();
            let ep = endpoint.to_string();
            let body = body.clone();
            let key = k.clone();
            let http = state.http.clone();
            async move {
                let result = call_brain(&http, &url, &ep, body).await;
                (key, result.unwrap_or_else(|e| json!({ "error": e })))
            }
        })
        .collect();

    futures::future::join_all(futures).await.into_iter().collect()
}

/// Thin adapter: pulls brain specs from the live DashMap and delegates to
/// the pure `routing::select_brains`. Kept here because `AppState` is
/// orchestrator-only; the pure function lives in `routing.rs`.
fn select_brains(state: &AppState, query: &str) -> Vec<String> {
    let specs: Vec<(String, Vec<String>)> = state
        .brains
        .iter()
        .map(|entry| (entry.key().clone(), entry.value().speciality.clone()))
        .collect();
    select_brains_pure(specs, query)
}

fn now_ts() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

// ─── Routes ─────────────────────────────────────────────────────────────────

async fn route_index() -> Json<Value> {
    Json(json!({
        "name":    "SoulLink Orchestrator V13.5 — Rust Native",
        "version": "13.5.0",
        "engine":  "axum 0.7 + tokio + dashmap + reqwest-pool",
        "features": ["turbulence-aware-routing", "parallel-calls", "auth-bearer",
                     "cohabitation-localhost", "domain-regex-validation",
                     "prometheus-metrics", "lock-free-registry"],
        "endpoints": [
            "GET  /",
            "GET  /api/mesh/status",
            "GET  /api/mesh/turbulence",
            "POST /api/mesh/query",
            "POST /api/mesh/think",
            "POST /api/mesh/reinforce",
            "POST /api/mesh/stimulate",
            "POST /api/mesh/spawn",
            "GET  /api/mesh/brains",
            "GET  /metrics",
            "POST /api/guardian/snapshot",
        ]
    }))
}

async fn route_status(State(state): State<AppState>) -> Json<Value> {
    let results = call_all_parallel(&state, "/api/stats", None, None).await;
    let mut mesh    = json!({});
    let mut total_n = 0u64;
    let mut online  = 0u32;

    let guardian_latest = state.guardian.get("latest").map(|g| g.value().clone());
    let gpu_temp: i32 = guardian_latest.as_ref()
        .and_then(|g| g.get("gpu_temp_c")).and_then(|v| v.as_f64()).unwrap_or(0.0) as i32;
    let throttle_active: bool = guardian_latest.as_ref()
        .and_then(|g| g.get("throttle_active")).and_then(|v| v.as_bool()).unwrap_or(false);
    let concurrency_limit: u32 = if throttle_active { 50 } else { 100 };

    for (key, r) in &results {
        let brain_cfg = state.brains.get(key);
        if r.get("error").is_some() {
            mesh[key] = json!({
                "state": "offline",
                "port": brain_cfg.map(|b| b.port).unwrap_or(0),
                "gpu_temp": gpu_temp,
                "throttle_active": throttle_active,
                "concurrency_limit": concurrency_limit,
            });
        } else {
            let n  = r.get("N").and_then(|v| v.as_u64()).unwrap_or(0);
            let hz = r.get("hz").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let turb = r.get("turbulence");
            let att = turb.and_then(|t| t.get("attractor")).and_then(|v| v.as_str()).unwrap_or("?");
            total_n += n;
            online  += 1;
            mesh[key] = json!({
                "state":     "online",
                "N":         n,
                "hz":        hz,
                "attractor": att,
                "port":      brain_cfg.map(|b| b.port).unwrap_or(0),
                "version":   "v13.5",
                "gpu_temp":  gpu_temp,
                "throttle_active": throttle_active,
                "concurrency_limit": concurrency_limit,
            });
        }
    }

    Json(json!({
        "ok":           true,
        "mesh":         mesh,
        "guardian":     guardian_latest.unwrap_or(json!({})),
        "total_N":      total_n,
        "online":       online,
        "total_brains": state.brains.len(),
        "version":      "v13.5",
        "ts":           now_ts(),
    }))
}

async fn route_turbulence(State(state): State<AppState>) -> Json<Value> {
    let results = call_all_parallel(&state, "/api/turbulence", None, None).await;
    let mut mesh_turb = json!({});
    let mut critical_brains: Vec<String> = vec![];
    let mut attractor_counts: HashMap<String, u32> = HashMap::new();

    for (key, r) in &results {
        if r.get("error").is_none() {
            let att  = r.get("attractor").and_then(|v| v.as_str()).unwrap_or("?").to_string();
            let val  = r.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let crit = r.get("is_critical").and_then(|v| v.as_bool()).unwrap_or(false);
            if crit { critical_brains.push(key.clone()); }
            *attractor_counts.entry(att.clone()).or_insert(0) += 1;
            mesh_turb[key] = json!({
                "value":     val,
                "critical":  crit,
                "attractor": att,
            });
        }
    }

    let best_brain = results.iter()
        .filter(|(_, r)| r.get("error").is_none())
        .max_by(|(_, a), (_, b)| {
            let sa = attractor_score(a.get("attractor").and_then(|v| v.as_str()).unwrap_or(""));
            let sb = attractor_score(b.get("attractor").and_then(|v| v.as_str()).unwrap_or(""));
            sa.partial_cmp(&sb).unwrap()
        })
        .map(|(k, _)| k.clone())
        .unwrap_or_default();

    Json(json!({
        "ok":                     true,
        "mesh":                   mesh_turb,
        "critical_brains":        critical_brains,
        "attractor_distribution": attractor_counts,
        "most_stable_brain":      best_brain,
        "ts":                     now_ts(),
    }))
}

async fn route_query(State(state): State<AppState>, Json(body): Json<Value>) -> Json<Value> {
    let question = body.get("question")
        .or_else(|| body.get("q"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if question.is_empty() {
        return Json(json!({"ok": false, "error": "question required"}));
    }

    state.query_count.fetch_add(1, Ordering::Relaxed);

    let mut selected = select_brains(&state, &question);
    let turb_results = call_all_parallel(&state, "/api/turbulence", None, Some(selected.clone())).await;

    selected.sort_by(|a, b| {
        let sa = turb_results.get(a).and_then(|r| r.get("attractor")).and_then(|v| v.as_str())
            .map(attractor_score).unwrap_or(0.5);
        let sb = turb_results.get(b).and_then(|r| r.get("attractor")).and_then(|v| v.as_str())
            .map(attractor_score).unwrap_or(0.5);
        sb.partial_cmp(&sa).unwrap()
    });

    let results = call_all_parallel(
        &state, "/api/query",
        Some(json!({"question": question})),
        Some(selected.clone()),
    ).await;

    let merged = merge_concepts(&results);
    let avg_mastery = if merged.is_empty() { 0.0 } else {
        merged.iter().filter_map(|c| c.get("mastery").and_then(|v| v.as_f64())).sum::<f64>()
            / merged.len() as f64
    };

    Json(json!({
        "ok":             true,
        "question":       question,
        "brains_queried": selected,
        "concepts_found": merged.len(),
        "top_concepts":   merged,
        "avg_mastery":    avg_mastery,
        "routing":        "turbulence-aware",
        "ts":             now_ts(),
    }))
}

async fn route_think(State(state): State<AppState>, Json(body): Json<Value>) -> Json<Value> {
    let task    = body.get("task").and_then(|v| v.as_str())
        .or_else(|| body.get("query").and_then(|v| v.as_str()))  // accept "query" from gateway too
        .unwrap_or("").to_string();
    let context = body.get("context").and_then(|v| v.as_str()).unwrap_or("").to_string();

    if task.is_empty() {
        return Json(json!({"ok": false, "error": "task required"}));
    }

    let selected = select_brains(&state, &format!("{} {}", task, context));
    let results = call_all_parallel(
        &state, "/api/query",
        Some(json!({"question": task})),
        Some(selected.clone()),
    ).await;

    let merged = merge_concepts(&results);
    let mesh_ctx = mesh_context::MeshContext::build(&task, &results, &merged);

    // ── Reply: try Ollama, fall back to symbolic synth ───────────────────
    let reply = match &state.ollama {
        Some(cfg) => match ollama_bridge::generate(cfg, &mesh_ctx).await {
            Ok(text) => text,
            Err(e) => {
                warn!(%e, "ollama unavailable — falling back to symbolic synth");
                synth_fallback::build(&mesh_ctx, Some("model temporarily unavailable"))
            }
        },
        None => synth_fallback::build(&mesh_ctx, None),
    };

    Json(json!({
        "ok":               true,
        "task":             task,
        "brains_consulted": selected,
        "concepts_found":   merged.len(),
        "top_concepts":     &merged[..merged.len().min(10)],
        "mesh_context":     mesh_ctx,
        "reply":            reply,
        "ts":               now_ts(),
    }))
}

/// Phase 6b: streaming variant of `/api/mesh/think`. Returns SSE events
/// with `event: meta` (sent once with brains_consulted, concepts_found,
/// mesh_context), followed by `event: token` frames with each Ollama
/// chunk, followed by `event: done` on clean completion or `event: error`.
///
/// Falls back to a single `event: token` with the symbolic synthesis if
/// Ollama is unavailable, so the gateway's stream consumer can always
/// count on seeing at least meta → token → done.
async fn route_stream(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>> {
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;

    let task = body.get("task").and_then(|v| v.as_str())
        .or_else(|| body.get("query").and_then(|v| v.as_str()))
        .unwrap_or("").to_string();
    let context = body.get("context").and_then(|v| v.as_str()).unwrap_or("").to_string();

    // Channel for SSE events from the producer task
    let (tx, rx) = mpsc::channel::<Result<Event, std::convert::Infallible>>(64);

    let ollama = state.ollama.clone();

    // Spawn the producer. It computes meta, then either streams Ollama
    // tokens or emits the fallback synthesis.
    tokio::spawn(async move {
        // Empty query → emit error immediately
        if task.is_empty() {
            let _ = tx.send(Ok(Event::default().event("error").data("task or query required"))).await;
            return;
        }

        let selected = select_brains(&state, &format!("{} {}", task, context));
        let results = call_all_parallel(
            &state, "/api/query",
            Some(json!({"question": task})),
            Some(selected.clone()),
        ).await;
        let merged = merge_concepts(&results);
        let mesh_ctx = mesh_context::MeshContext::build(&task, &results, &merged);

        // Send meta first
        let meta = json!({
            "brains_consulted": selected,
            "concepts_found":   merged.len(),
            "mesh_context":     mesh_ctx,
        });
        let meta_ev = Event::default()
            .event("meta")
            .json_data(&meta)
            .unwrap_or_else(|_| Event::default().event("meta").data("{}"));
        if tx.send(Ok(meta_ev)).await.is_err() {
            return; // client disconnected
        }

        // Stream tokens (or fallback)
        match ollama {
            Some(cfg) => {
                use futures::StreamExt;
                match ollama_bridge::generate_stream(&cfg, &mesh_ctx).await {
                    Ok(mut stream) => {
                        while let Some(chunk) = stream.next().await {
                            match chunk {
                                Ok(token) => {
                                    let ev = Event::default().event("token").data(token);
                                    if tx.send(Ok(ev)).await.is_err() {
                                        return;
                                    }
                                }
                                Err(e) => {
                                    let ev = Event::default().event("error").data(e.to_string());
                                    let _ = tx.send(Ok(ev)).await;
                                    return;
                                }
                            }
                        }
                        let _ = tx.send(Ok(Event::default().event("done").data("ok"))).await;
                    }
                    Err(e) => {
                        warn!(%e, "stream: ollama init failed, falling back to symbolic");
                        let text = synth_fallback::build(&mesh_ctx, Some("streaming unavailable"));
                        let _ = tx.send(Ok(Event::default().event("token").data(text))).await;
                        let _ = tx.send(Ok(Event::default().event("done").data("fallback"))).await;
                    }
                }
            }
            None => {
                let text = synth_fallback::build(&mesh_ctx, None);
                let _ = tx.send(Ok(Event::default().event("token").data(text))).await;
                let _ = tx.send(Ok(Event::default().event("done").data("no_llm"))).await;
            }
        }
    });

    Sse::new(ReceiverStream::new(rx)).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    )
}

async fn route_reinforce(State(state): State<AppState>, Json(body): Json<Value>) -> Json<Value> {
    let concept = body.get("concept").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let delta   = body.get("delta").and_then(|v| v.as_f64()).unwrap_or(0.05);
    let brains  = body.get("brains")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>());

    if concept.is_empty() {
        return Json(json!({"ok": false, "error": "concept required"}));
    }

    let selected = brains.unwrap_or_else(|| select_brains(&state, &concept));
    let results  = call_all_parallel(
        &state, "/api/reinforce",
        Some(json!({"concept": concept, "delta": delta})),
        Some(selected.clone()),
    ).await;

    let updated: Vec<_> = results.iter()
        .filter(|(_, r)| r.get("error").is_none())
        .map(|(k, r)| json!({"brain": k, "mastery": r.get("mastery")}))
        .collect();

    Json(json!({
        "ok":      true,
        "concept": concept,
        "delta":   delta,
        "updated": updated.len(),
        "results": updated,
    }))
}

async fn route_stimulate(State(state): State<AppState>, Json(body): Json<Value>) -> Json<Value> {
    let module   = body.get("module").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let strength = body.get("strength").and_then(|v| v.as_f64()).unwrap_or(0.5);
    let selected = select_brains(&state, &module);

    let results = call_all_parallel(
        &state, "/api/stimulate",
        Some(json!({"module": module, "strength": strength})),
        Some(selected.clone()),
    ).await;

    let ok_count = results.values().filter(|r| r.get("error").is_none()).count();

    Json(json!({
        "ok":         true,
        "module":     module,
        "strength":   strength,
        "stimulated": ok_count,
        "brains":     selected,
    }))
}

/// Spawn a new v12 brain — Phase 1 adds regex validation on `domain`.
async fn route_spawn(State(state): State<AppState>, Json(body): Json<Value>) -> Json<Value> {
    let domain = body.get("domain").and_then(|v| v.as_str()).unwrap_or("").to_string();

    // SECURITY: strict whitelist of brain names.
    // Previously any string flowed through to `python3 … --brain <domain>`;
    // while `Command::arg` prevents shell injection, it did not prevent path
    // traversal or garbage values creating bogus state. Tight regex closes
    // both gaps.
    if !validation::valid_brain_domain(&domain) {
        warn!(domain, "spawn rejected — invalid domain format");
        return Json(json!({
            "ok": false,
            "error": "domain must match ^[a-z][a-z0-9_]{1,31}$",
        }));
    }
    if state.brains.contains_key(&domain) {
        return Json(json!({"ok": false, "error": format!("brain '{}' already exists", domain)}));
    }

    let used_ports: Vec<u16> = state.brains.iter().map(|e| e.value().port).collect();
    let mut port = 9016u16;
    while used_ports.contains(&port) {
        port += 1;
    }

    let brain_dir = state.brain_dir.clone();
    let domain_c  = domain.clone();
    let brains    = state.brains.clone();
    let http      = state.http.clone();

    state.spawn_count.fetch_add(1, Ordering::Relaxed);

    tokio::spawn(async move {
        info!("[SPAWN] launching Brain-{} on :{}", domain_c, port);
        let result = Command::new("python3")
            .arg(format!("{}/brain_v12.py", brain_dir))
            .arg("--brain")
            .arg(&domain_c)
            .spawn();

        match result {
            Ok(_child) => {
                tokio::time::sleep(Duration::from_secs(8)).await;
                let url = format!("http://127.0.0.1:{}/api/stats", port);
                for _ in 0..6 {
                    match http.get(&url).timeout(Duration::from_secs(2)).send().await {
                        Ok(r) if r.status().is_success() => {
                            brains.insert(domain_c.clone(), BrainConfig {
                                port,
                                url: format!("http://127.0.0.1:{}", port),
                                speciality: vec![domain_c.clone()],
                                version: "v12".to_string(),
                                spawned_by: "orchestrator-v13.5".to_string(),
                            });
                            info!("[SPAWN] ✅ Brain-{} active on :{}", domain_c, port);
                            return;
                        }
                        _ => {}
                    }
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                warn!("[SPAWN] ⚠️ Brain-{} not responding", domain_c);
            }
            Err(e) => error!("[SPAWN] ❌ error: {}", e),
        }
    });

    Json(json!({
        "ok":     true,
        "domain": domain,
        "port":   port,
        "msg":    format!("spawning Brain-{} on :{}", domain, port),
        "check":  "/api/mesh/status after ~30s",
    }))
}

async fn route_brains(State(state): State<AppState>) -> Json<Value> {
    let brains: HashMap<String, Value> = state.brains.iter().map(|entry| {
        (entry.key().clone(), json!({
            "port":       entry.value().port,
            "speciality": entry.value().speciality,
            "version":    entry.value().version,
            "spawned_by": entry.value().spawned_by,
        }))
    }).collect();

    Json(json!({
        "ok":     true,
        "brains": brains,
        "total":  brains.len(),
    }))
}

// ─── Guardian snapshot receiver ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GuardianSnapshot {
    #[serde(default)] ts:                String,
    #[serde(default)] correlation_id:    String,
    #[serde(default)] gpu_temp_c:        f32,
    #[serde(default)] gpu_util_pct:      u32,
    #[serde(default)] gpu_mem_used_mib:  u64,
    #[serde(default)] gpu_mem_total_mib: u64,
    #[serde(default)] gpu_power_w:       f32,
    #[serde(default)] fan_speed_pct:     u32,
    #[serde(default)] sm_clock_mhz:      u32,
    #[serde(default)] mem_clock_mhz:     u32,
    #[serde(default)] xid_error_count:   u64,
    #[serde(default)] throttle_active:   bool,
    #[serde(default)] power_limit_w:     f32,
    #[serde(default)] throttle_reasons:  u64,
}

async fn route_guardian_snapshot(
    State(state): State<AppState>,
    Json(snap): Json<GuardianSnapshot>,
) -> Json<Value> {
    state.guardian.insert("latest".to_string(), json!({
        "ts":                snap.ts,
        "correlation_id":    snap.correlation_id,
        "gpu_temp_c":        snap.gpu_temp_c,
        "gpu_util_pct":      snap.gpu_util_pct,
        "gpu_mem_used_mib":  snap.gpu_mem_used_mib,
        "gpu_mem_total_mib": snap.gpu_mem_total_mib,
        "gpu_power_w":       snap.gpu_power_w,
        "power_limit_w":     snap.power_limit_w,
        "fan_speed_pct":     snap.fan_speed_pct,
        "sm_clock_mhz":      snap.sm_clock_mhz,
        "mem_clock_mhz":     snap.mem_clock_mhz,
        "throttle_active":   snap.throttle_active,
        "throttle_reasons":  snap.throttle_reasons,
        "xid_error_count":   snap.xid_error_count,
        "received_at":       now_ts(),
    }));

    if snap.throttle_active {
        let http = state.http.clone();
        let guardian = state.guardian.clone();
        tokio::spawn(async move {
            warn!("GPU THROTTLE — load shedding: Science/Mind low-power");
            let _ = http
                .post("http://127.0.0.1:9010/api/stimulate")
                .json(&json!({"count": 1}))
                .timeout(Duration::from_secs(2))
                .send()
                .await;
            let _ = http
                .post("http://127.0.0.1:9011/api/stimulate")
                .json(&json!({"count": 1}))
                .timeout(Duration::from_secs(2))
                .send()
                .await;
            guardian.insert("load_shedding".to_string(), json!({
                "active": true,
                "since":  now_ts(),
            }));
        });
    }

    info!(temp_c = snap.gpu_temp_c, throttle = snap.throttle_active, power_w = snap.gpu_power_w,
          "guardian snapshot received");
    Json(json!({"status": "ok", "stored": true}))
}

async fn route_metrics(State(state): State<AppState>) -> String {
    format!(
        "# SoulLink Orchestrator V13.5 metrics\n\
         soullink_queries_total {}\n\
         soullink_spawns_total {}\n\
         soullink_brains_registered {}\n",
        state.query_count.load(Ordering::Relaxed),
        state.spawn_count.load(Ordering::Relaxed),
        state.brains.len(),
    )
}

// ─── Main ───────────────────────────────────────────────────────────────────

#[tokio::main(worker_threads = 24)]
async fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();

    // Port / bind address
    let port = std::env::args().nth(1).and_then(|p| p.parse::<u16>().ok()).unwrap_or(9020);
    // Phase 1: default to loopback. Override explicitly via ORCH_BIND=0.0.0.0
    // (not recommended — the gateway binary is the external-facing front).
    let bind_host = std::env::var("ORCH_BIND").unwrap_or_else(|_| "127.0.0.1".to_string());

    let brain_dir = std::env::args().nth(2)
        .unwrap_or_else(|| "/mnt/nvme/soullink_brain".to_string());

    let mut state = AppState::new(&brain_dir);

    // ── Phase 6a-hybrid: load orchestrator.toml and wire Ollama bridge ──
    let orch_cfg = config::load_default();
    if let Some(ollama_cfg) = orch_cfg.ollama {
        info!(
            base_url = %ollama_cfg.base_url,
            model = %ollama_cfg.model,
            "Ollama bridge enabled"
        );
        state.ollama = Some(Arc::new(ollama_cfg));
    } else {
        info!("no [ollama] in orchestrator.toml — /api/mesh/think will return symbolic synth");
    }

    // ── Phase 3 UDS client to the Guardian ──────────────────────────────
    let ipc_path = std::env::var("SOULLINK_IPC_SOCKET")
        .unwrap_or_else(|_| soullink_core::ipc::DEFAULT_SOCKET_PATH.to_string());
    let ipc_client = ipc_client::IpcClient::new(
        &ipc_path,
        ipc_client::dashmap_sink(state.guardian.clone()),
    );
    tokio::spawn(async move { ipc_client.run().await });
    info!(ipc = %ipc_path, "UDS guardian snapshot consumer spawned");

    // ── Auth ──────────────────────────────────────────────────────────────
    //
    // Phase 6b-audit-triage v2: load the token from the environment
    // FIRST (where systemd injects it from EnvironmentFile=secrets.env),
    // and fall back to the file only if the env is empty. Reading the
    // file directly fails when the orchestrator runs as a non-root user
    // (soullink uid 991) and secrets.env is the mandated 0600 root:root
    // — which is exactly the production configuration. Pre-fix, this
    // caused the orchestrator to generate a RANDOM ephemeral token on
    // every start, leading to ~21 665 "auth failed — bad token" WARN/day
    // from the guardian (which has the right token via its own env).
    //
    // Fall-through order:
    //   1. $SOULLINK_INTERNAL_TOKEN   (systemd-injected, the happy path)
    //   2. secrets::load_default()    (file read, still works if perms
    //                                  allow and orchestrator runs as root)
    //   3. Ephemeral RNG              (cohabitation last resort; warn loud)
    let token = match std::env::var("SOULLINK_INTERNAL_TOKEN").ok()
        .filter(|s| !s.is_empty())
        .and_then(|hex| soullink_core::secrets::InternalToken::from_hex(&hex).ok())
    {
        Some(t) => {
            info!("internal token loaded from $SOULLINK_INTERNAL_TOKEN");
            t
        }
        None => match secrets::load_default() {
            Ok(t) => {
                info!("internal token loaded from /etc/soullink/secrets.env");
                t
            }
            Err(e) => {
                warn!(%e, "token unavailable from env AND file — generating ephemeral token \
                           (cohabitation mode; localhost still unauthed). \
                           This will break guardian auth; fix $SOULLINK_INTERNAL_TOKEN \
                           in EnvironmentFile=/etc/soullink/secrets.env");
                let mut bytes = [0u8; 32];
                let seed = now_ts() ^ std::process::id() as u64;
                for (i, b) in bytes.iter_mut().enumerate() {
                    *b = ((seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)) >> (i % 56)) as u8;
                }
                soullink_core::secrets::InternalToken(bytes)
            }
        },
    };

    let auth_cfg = AuthConfig::new(token, /* allow_localhost_unauthed: */ true);

    // ── Routes ────────────────────────────────────────────────────────────
    // Public-ish read routes: no auth (read-only data, safe on localhost).
    let public = Router::new()
        .route("/",                      get(route_index))
        .route("/api/mesh/status",       get(route_status))
        .route("/api/mesh/turbulence",   get(route_turbulence))
        .route("/api/mesh/brains",       get(route_brains))
        .route("/metrics",               get(route_metrics));

    // Sensitive routes: auth required (with cohabitation bypass for localhost).
    let protected = Router::new()
        .route("/api/mesh/query",        post(route_query))
        .route("/api/mesh/think",        post(route_think))
        .route("/api/mesh/stream",       post(route_stream))
        .route("/api/mesh/reinforce",    post(route_reinforce))
        .route("/api/mesh/stimulate",    post(route_stimulate))
        .route("/api/mesh/spawn",        post(route_spawn))
        .route("/api/guardian/snapshot", post(route_guardian_snapshot))
        .layer(middleware::from_fn_with_state(auth_cfg.clone(), require_auth));

    let app = public.merge(protected).with_state(state);

    let addr: SocketAddr = format!("{}:{}", bind_host, port)
        .parse()
        .expect("invalid bind address");
    info!("🚀 SoulLink Orchestrator V13.5 listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
