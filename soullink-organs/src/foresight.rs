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

const PORT: u16 = 9040;
const N: usize = 400;
const VERSION: &str = "v6.1";

#[derive(Serialize, Deserialize, Clone)]
struct Prediction {
    context: String,
    predicted_value: f64,
    confidence: f64,
    trend: String,
    timestamp: i64,
}

#[derive(Serialize, Deserialize, Clone)]
struct Pattern {
    name: String,
    period_hours: f64,
    strength: f64,
    last_seen: i64,
}

struct AppState {
    predictions: DashMap<String, Prediction>,
    patterns: DashMap<String, Pattern>,
    alpha: f64, // exponential smoothing
    tick_count: DashMap<String, u64>,
    spikes: DashMap<String, usize>,
    rewards: DashMap<String, usize>,
    start_time: Instant,
    last_stimulus: DashMap<String, Instant>,
}

fn default_state() -> Arc<AppState> {
    Arc::new(AppState {
        predictions: DashMap::new(),
        patterns: DashMap::new(),
        alpha: 0.3,
        tick_count: DashMap::new(),
        spikes: DashMap::new(),
        rewards: DashMap::new(),
        start_time: Instant::now(),
        last_stimulus: DashMap::new(),
    })
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
    let _variance = tv * 0.1;
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
        "N": N, "avg_weight": 0.5, "concepts_count": s.predictions.len(), "hz": hz,
        "n": N, "name": "foresight", "rewards_count": rw, "spikes": sp, "tick_count": tc,
        "turbulence": {"attractor": att, "is_critical": crit, "mean_gradient": 0.01, "value": tv, "variance": tv * 0.1},
        "version": VERSION
    }))
}

#[derive(Deserialize)]
struct PredictRequest {
    context: String,
    horizon_hours: Option<f64>,
}

async fn api_predict(
    State(s): State<Arc<AppState>>,
    Json(req): Json<PredictRequest>,
) -> Json<serde_json::Value> {
    *s.tick_count.entry("predict".to_string()).or_insert(0) += 1;
    *s.spikes.entry("predict".to_string()).or_insert(0) += 1;
    s.last_stimulus
        .insert("predict".to_string(), Instant::now());

    let horizon = req.horizon_hours.unwrap_or(24.0);
    // Exponential smoothing prediction
    let existing = s.predictions.get(&req.context);
    let (predicted, confidence, trend) = if let Some(prev) = existing {
        let smoothed = prev.predicted_value * (1.0 - s.alpha) + prev.predicted_value * s.alpha;
        let t = if prev.predicted_value > smoothed {
            "up"
        } else if prev.predicted_value < smoothed {
            "down"
        } else {
            "stable"
        };
        (smoothed * (1.0 + 0.01 * horizon), 0.7, t.to_string())
    } else {
        (0.5, 0.3, "unknown".to_string())
    };

    let pred = Prediction {
        context: req.context.clone(),
        predicted_value: predicted,
        confidence,
        trend: trend.clone(),
        timestamp: chrono::Utc::now().timestamp_millis(),
    };
    s.predictions.insert(req.context.clone(), pred.clone());

    Json(
        serde_json::json!({"predicted_value": pred.predicted_value, "confidence": pred.confidence, "trend": pred.trend, "horizon_hours": horizon}),
    )
}

async fn api_patterns(State(s): State<Arc<AppState>>) -> Json<serde_json::Value> {
    *s.tick_count.entry("patterns".to_string()).or_insert(0) += 1;
    let patterns: Vec<serde_json::Value> = s.patterns.iter().map(|r| serde_json::json!({"name": r.key(), "period_hours": r.value().period_hours, "strength": r.value().strength})).collect();
    // Seed some default patterns if empty
    if patterns.is_empty() {
        Json(serde_json::json!({"patterns": [
            {"name": "circadian", "period_hours": 24.0, "strength": 0.8},
            {"name": "weekly", "period_hours": 168.0, "strength": 0.6},
            {"name": "hourly_burst", "period_hours": 1.0, "strength": 0.4}
        ]}))
    } else {
        Json(serde_json::json!({"patterns": patterns}))
    }
}

#[derive(Deserialize)]
struct PrefetchRequest {
    target: String,
    priority: Option<f64>,
}

async fn api_prefetch(
    State(s): State<Arc<AppState>>,
    Json(req): Json<PrefetchRequest>,
) -> Json<serde_json::Value> {
    *s.tick_count.entry("prefetch".to_string()).or_insert(0) += 1;
    *s.spikes.entry("prefetch".to_string()).or_insert(0) += 1;
    let priority = req.priority.unwrap_or(0.5);
    Json(
        serde_json::json!({"status": "prefetch_triggered", "target": req.target, "priority": priority, "estimated_latency_ms": 50}),
    )
}

async fn api_foresight_health(State(s): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(
        serde_json::json!({"status": "healthy", "predictions_count": s.predictions.len(), "patterns_count": s.patterns.len(), "uptime_secs": s.start_time.elapsed().as_secs()}),
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
    Json(serde_json::json!({"status": "stimulated", "organ": "foresight"}))
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct ReinforceRequest {
    reward: Option<f64>,
}

async fn api_reinforce(
    State(s): State<Arc<AppState>>,
    Json(_req): Json<ReinforceRequest>,
) -> Json<serde_json::Value> {
    *s.rewards.entry("reinforce".to_string()).or_insert(0) += 1;
    *s.spikes.entry("reinforce".to_string()).or_insert(0) += 1;
    Json(serde_json::json!({"status": "reinforced", "organ": "foresight"}))
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct LearnRequest {
    concept: Option<String>,
    data: Option<serde_json::Value>,
}

async fn api_learn(
    State(s): State<Arc<AppState>>,
    Json(req): Json<LearnRequest>,
) -> Json<serde_json::Value> {
    *s.spikes.entry("learn".to_string()).or_insert(0) += 1;
    if let Some(name) = &req.concept {
        let p = Pattern {
            name: name.clone(),
            period_hours: 24.0,
            strength: 0.5,
            last_seen: chrono::Utc::now().timestamp_millis(),
        };
        s.patterns.insert(name.clone(), p);
    }
    Json(serde_json::json!({"status": "learned", "organ": "foresight"}))
}

#[derive(Deserialize)]
struct QueryRequest {
    query: String,
    top_k: Option<usize>,
}

async fn api_query(
    State(s): State<Arc<AppState>>,
    Json(req): Json<QueryRequest>,
) -> Json<serde_json::Value> {
    *s.spikes.entry("query".to_string()).or_insert(0) += 1;
    let results: Vec<serde_json::Value> = s.predictions.iter()
        .filter(|r| r.key().contains(&req.query) || r.value().context.contains(&req.query))
        .take(req.top_k.unwrap_or(5))
        .map(|r| serde_json::json!({"context": r.value().context, "predicted_value": r.value().predicted_value, "confidence": r.value().confidence, "trend": r.value().trend}))
        .collect();
    Json(serde_json::json!({"results": results, "organ": "foresight"}))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    let state = default_state();
    let app = Router::new()
        .route("/api/stats", get(api_stats))
        .route("/api/foresight/predict", post(api_predict))
        .route("/api/foresight/patterns", get(api_patterns))
        .route("/api/foresight/prefetch", post(api_prefetch))
        .route("/api/foresight/health", get(api_foresight_health))
        .route("/api/stimulate", post(api_stimulate))
        .route("/api/reinforce", post(api_reinforce))
        .route("/api/learn", post(api_learn))
        .route("/api/query", post(api_query))
        .with_state(state);
    let listener = TcpListener::bind(format!("0.0.0.0:{}", PORT))
        .await
        .unwrap();
    tracing::info!(
        "soullink-foresight v{} listening on port {} (N={})",
        VERSION,
        PORT,
        N
    );
    axum::serve(listener, app).await.unwrap();
}
