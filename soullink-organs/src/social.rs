use axum::{Json, Router, extract::State, routing::{get, post}};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpListener;

const PORT: u16 = 9043;
const N: usize = 300;
const VERSION: &str = "v6.1";

#[derive(Serialize, Deserialize, Clone)]
struct Profile {
    id: String,
    formality: String, // informal, neutral, formal
    intent: String,    // question, command, social, work
    emotional_state: String,
    preferences: Vec<String>,
    interaction_count: u64,
    last_seen: i64,
}

#[derive(Serialize, Deserialize, Clone)]
struct SocialContext {
    group_size: usize,
    dominant_formality: String,
    active_speakers: Vec<String>,
    mood: String,
    hierarchy: String, // flat, structured, unknown
}

struct AppState {
    profiles: DashMap<String, Profile>,
    context: DashMap<String, SocialContext>,
    tick_count: DashMap<String, u64>,
    spikes: DashMap<String, usize>,
    rewards: DashMap<String, usize>,
    start_time: Instant,
    last_stimulus: DashMap<String, Instant>,
}

fn default_state() -> Arc<AppState> {
    Arc::new(AppState {
        profiles: DashMap::new(),
        context: DashMap::new(),
        tick_count: DashMap::new(),
        spikes: DashMap::new(),
        rewards: DashMap::new(),
        start_time: Instant::now(),
        last_stimulus: DashMap::new(),
    })
}

fn calc_turbulence(state: &AppState) -> (f64, String, bool) {
    let activity: f64 = state.spikes.iter().map(|r| *r.value() as f64).sum::<f64>().min(100.0) / 100.0;
    let tv = 1.0 - activity;
    let attractor = if tv < 0.05 { "DeepBasin" } else if tv < 0.2 { "StableOrbit" } else if tv < 0.5 { "Transient" } else { "StrangeAttractor" };
    (tv.min(1.0), attractor.to_string(), tv > 0.7)
}

fn detect_formality(text: &str) -> String {
    let lower = text.to_lowercase();
    let formal_markers = ["s'il vous plaît", "merci", "cordialement", "respectueusement", "please", "sincerely"];
    let informal_markers = ["salut", "yo", "cool", "super", "ok", "bjr", "slt", "hey"];
    let formal_count = formal_markers.iter().filter(|m| lower.contains(*m)).count();
    let informal_count = informal_markers.iter().filter(|m| lower.contains(*m)).count();
    if formal_count > informal_count { "formal" } else if informal_count > formal_count { "informal" } else { "neutral" }.to_string()
}

fn detect_intent(text: &str) -> String {
    let lower = text.to_lowercase();
    if lower.contains("?") || lower.starts_with("comment") || lower.starts_with("pourquoi") || lower.starts_with("what") || lower.starts_with("how") { "question" }
    else if lower.starts_with("fais") || lower.starts_with("do ") || lower.starts_with("run") || lower.starts_with("create") || lower.starts_with("exécute") { "command" }
    else if lower.contains("merci") || lower.contains("bravo") || lower.contains("super") { "social" }
    else { "work" }.to_string()
}

async fn api_stats(State(s): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let tc: u64 = s.tick_count.iter().map(|r| *r.value()).sum();
    let sp: usize = s.spikes.iter().map(|r| *r.value()).sum();
    let rw: usize = s.rewards.iter().map(|r| *r.value()).sum();
    let (tv, att, crit) = calc_turbulence(&s);
    let hz = if s.start_time.elapsed().as_secs() > 0 { sp as f64 / s.start_time.elapsed().as_secs() as f64 } else { 0.0 };
    Json(serde_json::json!({
        "N": N, "avg_weight": 0.5, "concepts_count": s.profiles.len(), "hz": hz,
        "n": N, "name": "social", "rewards_count": rw, "spikes": sp, "tick_count": tc,
        "turbulence": {"attractor": att, "is_critical": crit, "mean_gradient": 0.01, "value": tv, "variance": tv * 0.1},
        "version": VERSION
    }))
}

#[derive(Deserialize)]
struct ModelRequest { speaker_id: String, text: Option<String> }

async fn api_model(State(s): State<Arc<AppState>>, Json(req): Json<ModelRequest>) -> Json<serde_json::Value> {
    *s.tick_count.entry("model".to_string()).or_insert(0) += 1;
    *s.spikes.entry("model".to_string()).or_insert(0) += 1;
    let text = req.text.unwrap_or_default();
    let formality = detect_formality(&text);
    let intent = detect_intent(&text);
    // Update or create profile
    let mut profile = s.profiles.entry(req.speaker_id.clone()).or_insert(Profile {
        id: req.speaker_id.clone(), formality: formality.clone(), intent: intent.clone(),
        emotional_state: "neutral".to_string(), preferences: Vec::new(), interaction_count: 0,
        last_seen: chrono::Utc::now().timestamp_millis(),
    });
    profile.interaction_count += 1;
    profile.formality = formality.clone();
    profile.intent = intent.clone();
    profile.last_seen = chrono::Utc::now().timestamp_millis();
    Json(serde_json::json!({"speaker_id": req.speaker_id, "formality": profile.formality, "intent": profile.intent, "emotional_state": profile.emotional_state, "interaction_count": profile.interaction_count}))
}

#[derive(Deserialize)]
struct AdaptRequest { speaker_id: String, _message: String }

async fn api_adapt(State(s): State<Arc<AppState>>, Json(req): Json<AdaptRequest>) -> Json<serde_json::Value> {
    *s.tick_count.entry("adapt".to_string()).or_insert(0) += 1;
    *s.spikes.entry("adapt".to_string()).or_insert(0) += 1;
    let profile = s.profiles.get(&req.speaker_id);
    let (tone, style) = if let Some(p) = profile {
        match p.formality.as_str() {
            "formal" => ("professional", "structured"),
            "informal" => ("casual", "concise"),
            _ => ("balanced", "natural"),
        }
    } else {
        ("balanced", "natural")
    };
    Json(serde_json::json!({"speaker_id": req.speaker_id, "recommended_tone": tone, "recommended_style": style, "formality_match": true}))
}

async fn api_social_context(State(s): State<Arc<AppState>>) -> Json<serde_json::Value> {
    *s.tick_count.entry("context".to_string()).or_insert(0) += 1;
    let ctx = s.context.get("current").map(|r| r.value().clone());
    if let Some(ctx) = ctx {
        Json(serde_json::json!({"group_size": ctx.group_size, "dominant_formality": ctx.dominant_formality, "active_speakers": ctx.active_speakers, "mood": ctx.mood, "hierarchy": ctx.hierarchy}))
    } else {
        Json(serde_json::json!({"group_size": 1, "dominant_formality": "neutral", "active_speakers": [], "mood": "neutral", "hierarchy": "flat"}))
    }
}

#[derive(Deserialize)]
struct ProfileUpdate { speaker_id: String, formality: Option<String>, preferences: Option<Vec<String>> }

async fn api_profile(State(s): State<Arc<AppState>>, Json(req): Json<ProfileUpdate>) -> Json<serde_json::Value> {
    *s.tick_count.entry("profile".to_string()).or_insert(0) += 1;
    *s.spikes.entry("profile".to_string()).or_insert(0) += 1;
    let mut profile = s.profiles.entry(req.speaker_id.clone()).or_insert(Profile {
        id: req.speaker_id.clone(), formality: "neutral".to_string(), intent: "unknown".to_string(),
        emotional_state: "neutral".to_string(), preferences: Vec::new(), interaction_count: 0,
        last_seen: chrono::Utc::now().timestamp_millis(),
    });
    if let Some(f) = &req.formality { profile.formality = f.clone(); }
    if let Some(p) = &req.preferences { profile.preferences = p.clone(); }
    profile.last_seen = chrono::Utc::now().timestamp_millis();
    Json(serde_json::json!({"status": "updated", "speaker_id": req.speaker_id, "formality": profile.formality, "preferences": profile.preferences}))
}

// Standard mesh endpoints
#[derive(Deserialize)]
struct StimulusRequest { _content: Option<String>, _intensity: Option<f64> }

async fn api_stimulate(State(s): State<Arc<AppState>>, Json(_req): Json<StimulusRequest>) -> Json<serde_json::Value> {
    *s.spikes.entry("stimulate".to_string()).or_insert(0) += 1;
    s.last_stimulus.insert("stimulate".to_string(), Instant::now());
    Json(serde_json::json!({"status": "stimulated", "organ": "social"}))
}

async fn api_reinforce(State(s): State<Arc<AppState>>, Json(_req): Json<serde_json::Value>) -> Json<serde_json::Value> {
    *s.rewards.entry("reinforce".to_string()).or_insert(0) += 1;
    *s.spikes.entry("reinforce".to_string()).or_insert(0) += 1;
    Json(serde_json::json!({"status": "reinforced", "organ": "social"}))
}

async fn api_learn(State(s): State<Arc<AppState>>, Json(_req): Json<serde_json::Value>) -> Json<serde_json::Value> {
    *s.spikes.entry("learn".to_string()).or_insert(0) += 1;
    Json(serde_json::json!({"status": "learned", "organ": "social"}))
}

async fn api_query(State(s): State<Arc<AppState>>, Json(req): Json<serde_json::Value>) -> Json<serde_json::Value> {
    *s.spikes.entry("query".to_string()).or_insert(0) += 1;
    let query = req.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let results: Vec<serde_json::Value> = s.profiles.iter()
        .filter(|r| r.key().contains(query) || r.value().id.contains(query))
        .take(5)
        .map(|r| serde_json::json!({"id": r.value().id, "formality": r.value().formality, "intent": r.value().intent, "interaction_count": r.value().interaction_count}))
        .collect();
    Json(serde_json::json!({"results": results, "organ": "social"}))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    let state = default_state();
    let app = Router::new()
        .route("/api/stats", get(api_stats))
        .route("/api/social/model", post(api_model))
        .route("/api/social/adapt", post(api_adapt))
        .route("/api/social/context", get(api_social_context))
        .route("/api/social/profile", post(api_profile))
        .route("/api/stimulate", post(api_stimulate))
        .route("/api/reinforce", post(api_reinforce))
        .route("/api/learn", post(api_learn))
        .route("/api/query", post(api_query))
        .with_state(state);
    let listener = TcpListener::bind(format!("0.0.0.0:{}", PORT)).await.unwrap();
    tracing::info!("soullink-social v{} listening on port {} (N={})", VERSION, PORT, N);
    axum::serve(listener, app).await.unwrap();
}