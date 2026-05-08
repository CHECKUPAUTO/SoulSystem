use axum::{Json, Router, extract::State, routing::{get, post}};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpListener;

const PORT: u16 = 9042;
const N: usize = 600;
const VERSION: &str = "v6.1";

#[derive(Serialize, Deserialize, Clone)]
struct Concept {
    name: String,
    domain: String,
    features: Vec<f64>,
    associations: Vec<String>,
    salience: f64,
}

#[derive(Serialize, Deserialize, Clone)]
struct CreativeOutput {
    idea: String,
    novelty: f64,
    coherence: f64,
    domains: Vec<String>,
    timestamp: i64,
}

struct AppState {
    concepts: DashMap<String, Concept>,
    outputs: DashMap<String, CreativeOutput>,
    tick_count: DashMap<String, u64>,
    spikes: DashMap<String, usize>,
    rewards: DashMap<String, usize>,
    start_time: Instant,
    last_stimulus: DashMap<String, Instant>,
}

fn default_state() -> Arc<AppState> {
    let s = Arc::new(AppState {
        concepts: DashMap::new(),
        outputs: DashMap::new(),
        tick_count: DashMap::new(),
        spikes: DashMap::new(),
        rewards: DashMap::new(),
        start_time: Instant::now(),
        last_stimulus: DashMap::new(),
    });
    // Seed concepts across domains
    let seeds = [
        ("neural_pathway", "neuroscience"), ("market_cycle", "economics"), ("harmony", "music"),
        ("fractal", "mathematics"), ("ecosystem", "biology"), ("algorithm", "computer_science"),
        ("metaphor", "linguistics"), ("resonance", "physics"), ("evolution", "biology"),
        ("pattern", "mathematics"), ("rhythm", "music"), ("entropy", "physics"),
    ];
    for (name, domain) in seeds {
        let features: Vec<f64> = (0..8).map(|i| ((name.len() + i) as f64 * 0.1).sin()).collect();
        s.concepts.insert(name.to_string(), Concept { name: name.to_string(), domain: domain.to_string(), features, associations: Vec::new(), salience: 0.5 });
    }
    s
}

fn calc_turbulence(state: &AppState) -> (f64, String, bool) {
    let activity: f64 = state.spikes.iter().map(|r| *r.value() as f64).sum::<f64>().min(100.0) / 100.0;
    let tv = 1.0 - activity;
    // Creativity organ favors StrangeAttractor
    let attractor = if tv < 0.05 { "DeepBasin" } else if tv < 0.15 { "StableOrbit" } else if tv < 0.35 { "Transient" } else { "StrangeAttractor" };
    (tv.min(1.0), attractor.to_string(), tv > 0.5)
}

fn cosine_sim(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() { return 0.0; }
    let dot: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm_a > 0.0 && norm_b > 0.0 { dot / (norm_a * norm_b) } else { 0.0 }
}

async fn api_stats(State(s): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let tc: u64 = s.tick_count.iter().map(|r| *r.value()).sum();
    let sp: usize = s.spikes.iter().map(|r| *r.value()).sum();
    let rw: usize = s.rewards.iter().map(|r| *r.value()).sum();
    let (tv, att, crit) = calc_turbulence(&s);
    let hz = if s.start_time.elapsed().as_secs() > 0 { sp as f64 / s.start_time.elapsed().as_secs() as f64 } else { 0.0 };
    Json(serde_json::json!({
        "N": N, "avg_weight": 0.5, "concepts_count": s.concepts.len(), "hz": hz,
        "n": N, "name": "creativity", "rewards_count": rw, "spikes": sp, "tick_count": tc,
        "turbulence": {"attractor": att, "is_critical": crit, "mean_gradient": 0.01, "value": tv, "variance": tv * 0.1},
        "version": VERSION
    }))
}

#[derive(Deserialize)]
struct CombineRequest { concept_a: String, concept_b: String, _method: Option<String> }

async fn api_combine(State(s): State<Arc<AppState>>, Json(req): Json<CombineRequest>) -> Json<serde_json::Value> {
    *s.tick_count.entry("combine".to_string()).or_insert(0) += 1;
    *s.spikes.entry("combine".to_string()).or_insert(0) += 1;
    let ca = s.concepts.get(&req.concept_a);
    let cb = s.concepts.get(&req.concept_b);
    let (novelty, idea) = match (ca, cb) {
        (Some(a), Some(b)) => {
            let sim = cosine_sim(&a.features, &b.features);
            let nov = 1.0 - sim; // more different = more novel
            let idea = format!("{}_{}", a.name, b.name);
            (nov, idea)
        }
        _ => (0.5, format!("{}_{}", req.concept_a, req.concept_b)),
    };
    let output = CreativeOutput { idea: idea.clone(), novelty, coherence: 1.0 - novelty * 0.3, domains: vec![req.concept_a.clone(), req.concept_b.clone()], timestamp: chrono::Utc::now().timestamp_millis() };
    s.outputs.insert(idea.clone(), output.clone());
    Json(serde_json::json!({"idea": output.idea, "novelty": output.novelty, "coherence": output.coherence, "domains": output.domains}))
}

#[derive(Deserialize)]
struct MetaphorRequest { source: String, target: String }

async fn api_metaphor(State(s): State<Arc<AppState>>, Json(req): Json<MetaphorRequest>) -> Json<serde_json::Value> {
    *s.tick_count.entry("metaphor".to_string()).or_insert(0) += 1;
    *s.spikes.entry("metaphor".to_string()).or_insert(0) += 1;
    // Generate metaphor by mapping source features to target domain
    let metaphor = format!("{} is the {} of {}", req.source, req.source, req.target);
    let novelty = 0.6 + (req.source.len() as f64 * 0.01).min(0.3);
    Json(serde_json::json!({"metaphor": metaphor, "novelty": novelty, "source": req.source, "target": req.target}))
}

#[derive(Deserialize)]
struct DivergeRequest { seed: String, steps: Option<usize> }

async fn api_diverge(State(s): State<Arc<AppState>>, Json(req): Json<DivergeRequest>) -> Json<serde_json::Value> {
    *s.tick_count.entry("diverge".to_string()).or_insert(0) += 1;
    *s.spikes.entry("diverge".to_string()).or_insert(0) += 1;
    let steps = req.steps.unwrap_or(5);
    let mut path = vec![req.seed.clone()];
    let mut current = req.seed.clone();
    for _ in 0..steps {
        // Random walk: find most dissimilar concept
        let curr_concept = s.concepts.get(&current);
        let mut best_next = current.clone();
        let mut best_dist = 1.0;
        for entry in s.concepts.iter() {
            if entry.key() == &current { continue; }
            if let Some(cc) = &curr_concept {
                let sim = cosine_sim(&cc.features, &entry.value().features);
                if sim < best_dist { best_dist = sim; best_next = entry.key().clone(); }
            } else {
                best_next = entry.key().clone();
                break;
            }
        }
        path.push(best_next.clone());
        current = best_next;
    }
    Json(serde_json::json!({"path": path, "steps": steps, "seed": req.seed}))
}

#[derive(Deserialize)]
struct EvaluateRequest { idea: String }

async fn api_evaluate(State(s): State<Arc<AppState>>, Json(req): Json<EvaluateRequest>) -> Json<serde_json::Value> {
    *s.tick_count.entry("evaluate".to_string()).or_insert(0) += 1;
    if let Some(output) = s.outputs.get(&req.idea) {
        Json(serde_json::json!({"idea": output.idea, "novelty": output.novelty, "coherence": output.coherence, "fitness": output.novelty * 0.6 + output.coherence * 0.4}))
    } else {
        Json(serde_json::json!({"idea": req.idea, "novelty": 0.5, "coherence": 0.5, "fitness": 0.5}))
    }
}

// Standard mesh endpoints
#[derive(Deserialize)]
struct StimulusRequest { _content: Option<String>, _intensity: Option<f64> }

async fn api_stimulate(State(s): State<Arc<AppState>>, Json(_req): Json<StimulusRequest>) -> Json<serde_json::Value> {
    *s.spikes.entry("stimulate".to_string()).or_insert(0) += 1;
    s.last_stimulus.insert("stimulate".to_string(), Instant::now());
    Json(serde_json::json!({"status": "stimulated", "organ": "creativity"}))
}

async fn api_reinforce(State(s): State<Arc<AppState>>, Json(_req): Json<serde_json::Value>) -> Json<serde_json::Value> {
    *s.rewards.entry("reinforce".to_string()).or_insert(0) += 1;
    *s.spikes.entry("reinforce".to_string()).or_insert(0) += 1;
    Json(serde_json::json!({"status": "reinforced", "organ": "creativity"}))
}

async fn api_learn(State(s): State<Arc<AppState>>, Json(req): Json<serde_json::Value>) -> Json<serde_json::Value> {
    *s.spikes.entry("learn".to_string()).or_insert(0) += 1;
    if let Some(name) = req.get("concept").and_then(|v| v.as_str()) {
        let domain = req.get("domain").and_then(|v| v.as_str()).unwrap_or("unknown");
        let features: Vec<f64> = (0..8).map(|i| ((name.len() + i) as f64 * 0.1).sin()).collect();
        s.concepts.insert(name.to_string(), Concept { name: name.to_string(), domain: domain.to_string(), features, associations: Vec::new(), salience: 0.5 });
    }
    Json(serde_json::json!({"status": "learned", "organ": "creativity"}))
}

async fn api_query(State(s): State<Arc<AppState>>, Json(req): Json<serde_json::Value>) -> Json<serde_json::Value> {
    *s.spikes.entry("query".to_string()).or_insert(0) += 1;
    let query = req.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let results: Vec<serde_json::Value> = s.concepts.iter()
        .filter(|r| r.key().contains(query) || r.value().domain.contains(query))
        .take(5)
        .map(|r| serde_json::json!({"name": r.value().name, "domain": r.value().domain, "salience": r.value().salience}))
        .collect();
    Json(serde_json::json!({"results": results, "organ": "creativity"}))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    let state = default_state();
    let app = Router::new()
        .route("/api/stats", get(api_stats))
        .route("/api/creativity/combine", post(api_combine))
        .route("/api/creativity/metaphor", post(api_metaphor))
        .route("/api/creativity/diverge", post(api_diverge))
        .route("/api/creativity/evaluate", post(api_evaluate))
        .route("/api/stimulate", post(api_stimulate))
        .route("/api/reinforce", post(api_reinforce))
        .route("/api/learn", post(api_learn))
        .route("/api/query", post(api_query))
        .with_state(state);
    let listener = TcpListener::bind(format!("0.0.0.0:{}", PORT)).await.unwrap();
    tracing::info!("soullink-creativity v{} listening on port {} (N={})", VERSION, PORT, N);
    axum::serve(listener, app).await.unwrap();
}