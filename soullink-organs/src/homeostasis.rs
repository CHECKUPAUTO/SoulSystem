use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpListener;

const PORT: u16 = 9041;
const N: usize = 200;
const VERSION: &str = "v6.1";

#[derive(Serialize, Deserialize, Clone)]
struct Vitals {
    cpu_usage: f64,
    memory_usage: f64,
    active_organs: usize,
    total_organs: usize,
    avg_latency_ms: f64,
    queue_depth: usize,
    timestamp: i64,
}

#[derive(Serialize, Deserialize, Clone)]
struct Threshold {
    metric: String,
    min: f64,
    max: f64,
    current: f64,
}

struct AppState {
    vitals: DashMap<String, Vitals>,
    thresholds: DashMap<String, Threshold>,
    tick_count: DashMap<String, u64>,
    spikes: DashMap<String, usize>,
    rewards: DashMap<String, usize>,
    start_time: Instant,
    last_stimulus: DashMap<String, Instant>,
}

fn default_state() -> Arc<AppState> {
    let s = Arc::new(AppState {
        vitals: DashMap::new(),
        thresholds: DashMap::new(),
        tick_count: DashMap::new(),
        spikes: DashMap::new(),
        rewards: DashMap::new(),
        start_time: Instant::now(),
        last_stimulus: DashMap::new(),
    });
    // Default thresholds
    s.thresholds.insert(
        "cpu".to_string(),
        Threshold {
            metric: "cpu".into(),
            min: 0.0,
            max: 80.0,
            current: 0.0,
        },
    );
    s.thresholds.insert(
        "memory".to_string(),
        Threshold {
            metric: "memory".into(),
            min: 0.0,
            max: 90.0,
            current: 0.0,
        },
    );
    s.thresholds.insert(
        "latency".to_string(),
        Threshold {
            metric: "latency".into(),
            min: 0.0,
            max: 500.0,
            current: 0.0,
        },
    );
    s.thresholds.insert(
        "queue_depth".to_string(),
        Threshold {
            metric: "queue_depth".into(),
            min: 0.0,
            max: 100.0,
            current: 0.0,
        },
    );
    s
}

fn calc_turbulence(state: &AppState) -> (f64, String, bool) {
    let activity: f64 = state
        .spikes
        .iter()
        .map(|r| *r.value() as f64)
        .sum::<f64>()
        .min(100.0)
        / 100.0;
    let tv = 1.0 - activity;
    let attractor = if tv < 0.05 {
        "DeepBasin"
    } else if tv < 0.2 {
        "StableOrbit"
    } else if tv < 0.5 {
        "Transient"
    } else {
        "StrangeAttractor"
    };
    (tv.min(1.0), attractor.to_string(), tv > 0.7)
}

async fn api_stats(State(s): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let tc: u64 = s.tick_count.iter().map(|r| *r.value()).sum();
    let sp: usize = s.spikes.iter().map(|r| *r.value()).sum();
    let rw: usize = s.rewards.iter().map(|r| *r.value()).sum();
    let (tv, att, crit) = calc_turbulence(&s);
    let hz = if s.start_time.elapsed().as_secs() > 0 {
        sp as f64 / s.start_time.elapsed().as_secs() as f64
    } else {
        0.0
    };
    Json(serde_json::json!({
        "N": N, "avg_weight": 0.5, "concepts_count": s.vitals.len(), "hz": hz,
        "n": N, "name": "homeostasis", "rewards_count": rw, "spikes": sp, "tick_count": tc,
        "turbulence": {"attractor": att, "is_critical": crit, "mean_gradient": 0.01, "value": tv, "variance": tv * 0.1},
        "version": VERSION
    }))
}

async fn api_vitals(State(s): State<Arc<AppState>>) -> Json<serde_json::Value> {
    *s.tick_count.entry("vitals".to_string()).or_insert(0) += 1;
    *s.spikes.entry("vitals".to_string()).or_insert(0) += 1;
    // Collect real vitals from /proc
    let cpu = read_cpu_usage();
    let mem = read_mem_usage();
    let v = Vitals {
        cpu_usage: cpu,
        memory_usage: mem,
        active_organs: 0,
        total_organs: 13,
        avg_latency_ms: 10.0,
        queue_depth: 0,
        timestamp: chrono::Utc::now().timestamp_millis(),
    };
    // Update threshold current values
    if let Some(mut t) = s.thresholds.get_mut("cpu") {
        t.current = cpu;
    }
    if let Some(mut t) = s.thresholds.get_mut("memory") {
        t.current = mem;
    }
    s.vitals.insert("current".to_string(), v.clone());
    Json(
        serde_json::json!({"cpu_usage": v.cpu_usage, "memory_usage": v.memory_usage, "active_organs": v.active_organs, "total_organs": v.total_organs, "avg_latency_ms": v.avg_latency_ms, "queue_depth": v.queue_depth}),
    )
}

fn read_cpu_usage() -> f64 {
    let Ok(content) = std::fs::read_to_string("/proc/loadavg") else {
        return 0.0;
    };
    let parts: Vec<&str> = content.split_whitespace().collect();
    parts
        .first()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0)
        * 100.0
        / 8.0 // 8 cores
}

fn read_mem_usage() -> f64 {
    let Ok(content) = std::fs::read_to_string("/proc/meminfo") else {
        return 0.0;
    };
    let mut total: f64 = 0.0;
    let mut available: f64 = 0.0;
    for line in content.lines() {
        if line.starts_with("MemTotal:") {
            total = line
                .split_whitespace()
                .nth(1)
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
        }
        if line.starts_with("MemAvailable:") {
            available = line
                .split_whitespace()
                .nth(1)
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
        }
    }
    if total > 0.0 {
        ((total - available) / total) * 100.0
    } else {
        0.0
    }
}

async fn api_regulate(State(s): State<Arc<AppState>>) -> Json<serde_json::Value> {
    *s.tick_count.entry("regulate".to_string()).or_insert(0) += 1;
    *s.spikes.entry("regulate".to_string()).or_insert(0) += 1;
    let cpu = read_cpu_usage();
    let mem = read_mem_usage();
    let mut actions = Vec::new();
    if cpu > 80.0 {
        actions.push("throttle_cpu");
    }
    if mem > 90.0 {
        actions.push("throttle_memory");
    }
    Json(serde_json::json!({"status": "regulated", "actions": actions, "cpu": cpu, "memory": mem}))
}

async fn api_thresholds(State(s): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let thresholds: Vec<serde_json::Value> = s.thresholds.iter().map(|r| serde_json::json!({"metric": r.value().metric, "min": r.value().min, "max": r.value().max, "current": r.value().current})).collect();
    Json(serde_json::json!({"thresholds": thresholds}))
}

async fn api_recover(State(s): State<Arc<AppState>>) -> Json<serde_json::Value> {
    *s.tick_count.entry("recover".to_string()).or_insert(0) += 1;
    *s.spikes.entry("recover".to_string()).or_insert(0) += 1;
    // Check which organs are offline
    let organs = [
        ("science", 9010),
        ("mind", 9011),
        ("engineer", 9012),
        ("crypto", 9013),
        ("creative", 9014),
        ("meta", 9015),
        ("memory", 9030),
        ("reflex", 9035),
        ("integration", 9032),
        ("perception", 9033),
        ("affect", 9034),
        ("language", 9036),
        ("reasoning", 9031),
    ];
    let mut offline = Vec::new();
    for (name, port) in &organs {
        let url = format!("http://127.0.0.1:{}/api/stats", port);
        if minreq::get(&url).send().is_err() {
            offline.push(*name);
        }
    }
    Json(
        serde_json::json!({"status": "recovery_check", "offline_organs": offline, "total_checked": organs.len()}),
    )
}

// Standard mesh endpoints
#[derive(Deserialize)]
#[allow(dead_code)]
struct StimulusRequest {
    content: Option<String>,
    intensity: Option<f64>,
}

async fn api_stimulate(
    State(s): State<Arc<AppState>>,
    Json(_req): Json<StimulusRequest>,
) -> Json<serde_json::Value> {
    *s.spikes.entry("stimulate".to_string()).or_insert(0) += 1;
    s.last_stimulus
        .insert("stimulate".to_string(), Instant::now());
    Json(serde_json::json!({"status": "stimulated", "organ": "homeostasis"}))
}

async fn api_reinforce(
    State(s): State<Arc<AppState>>,
    Json(_req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    *s.rewards.entry("reinforce".to_string()).or_insert(0) += 1;
    *s.spikes.entry("reinforce".to_string()).or_insert(0) += 1;
    Json(serde_json::json!({"status": "reinforced", "organ": "homeostasis"}))
}

async fn api_learn(
    State(s): State<Arc<AppState>>,
    Json(_req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    *s.spikes.entry("learn".to_string()).or_insert(0) += 1;
    Json(serde_json::json!({"status": "learned", "organ": "homeostasis"}))
}

async fn api_query(
    State(s): State<Arc<AppState>>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    *s.spikes.entry("query".to_string()).or_insert(0) += 1;
    let query = req.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let results: Vec<serde_json::Value> = s.vitals.iter()
        .filter(|r| r.key().contains(query))
        .map(|r| serde_json::json!({"key": r.key(), "cpu": r.value().cpu_usage, "mem": r.value().memory_usage}))
        .take(5)
        .collect();
    Json(serde_json::json!({"results": results, "organ": "homeostasis"}))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    let state = default_state();
    let app = Router::new()
        .route("/api/stats", get(api_stats))
        .route("/api/homeostasis/vitals", get(api_vitals))
        .route("/api/homeostasis/regulate", post(api_regulate))
        .route("/api/homeostasis/thresholds", get(api_thresholds))
        .route("/api/homeostasis/recover", post(api_recover))
        .route("/api/stimulate", post(api_stimulate))
        .route("/api/reinforce", post(api_reinforce))
        .route("/api/learn", post(api_learn))
        .route("/api/query", post(api_query))
        .with_state(state);
    let listener = TcpListener::bind(format!("0.0.0.0:{}", PORT))
        .await
        .unwrap();
    tracing::info!(
        "soullink-homeostasis v{} listening on port {} (N={})",
        VERSION,
        PORT,
        N
    );
    axum::serve(listener, app).await.unwrap();
}
