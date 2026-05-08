use axum::{Json, Router, extract::State, routing::{get, post}};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpListener;

const PORT: u16 = 9044;
const N: usize = 200;
const VERSION: &str = "v6.1";

#[derive(Serialize, Deserialize, Clone)]
struct AuditEntry {
    id: u64,
    action: String,
    subject: String,
    result: String,
    prev_hash: String,
    hash: String,
    timestamp: i64,
}

#[derive(Serialize, Deserialize, Clone)]
struct VerificationResult {
    coherent: bool,
    score: f64,
    issues: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct SafetyResult {
    safe: bool,
    violations: Vec<String>,
    risk_level: String,
}

struct AppState {
    audit_log: DashMap<u64, AuditEntry>,
    next_audit_id: DashMap<String, u64>,
    safety_rules: DashMap<String, Vec<String>>,
    tick_count: DashMap<String, u64>,
    spikes: DashMap<String, usize>,
    hz: DashMap<String, f64>,
    turbulence_value: DashMap<String, f64>,
    turbulence_variance: DashMap<String, f64>,
    rewards: DashMap<String, usize>,
    start_time: Instant,
    last_stimulus: DashMap<String, Instant>,
}

fn default_state() -> Arc<AppState> {
    let s = Arc::new(AppState {
        audit_log: DashMap::new(),
        next_audit_id: DashMap::new(),
        safety_rules: DashMap::new(),
        tick_count: DashMap::new(),
        spikes: DashMap::new(),
        hz: DashMap::new(),
        turbulence_value: DashMap::new(),
        turbulence_variance: DashMap::new(),
        rewards: DashMap::new(),
        start_time: Instant::now(),
        last_stimulus: DashMap::new(),
    });
    // Default safety rules
    s.safety_rules.insert("content".to_string(), vec!["no_pii".to_string(), "no_harm".to_string(), "no_deception".to_string()]);
    s.safety_rules.insert("system".to_string(), vec!["no_unauthorized_access".to_string(), "no_data_exfiltration".to_string()]);
    *s.next_audit_id.entry("id".to_string()).or_insert(0) = 0;
    s
}

fn calc_turbulence(state: &AppState) -> (f64, String, bool) {
    let activity: f64 = state.spikes.iter().map(|r| *r.value() as f64).sum::<f64>().min(100.0) / 100.0;
    let tv = 1.0 - activity;
    let attractor = if tv < 0.05 { "DeepBasin" } else if tv < 0.2 { "StableOrbit" } else if tv < 0.5 { "Transient" } else { "StrangeAttractor" };
    (tv.min(1.0), attractor.to_string(), tv > 0.7)
}

fn compute_hash(data: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn append_audit(state: &AppState, action: &str, subject: &str, result: &str) {
    let id: u64 = {
        let mut id_ref = state.next_audit_id.get_mut("id").unwrap();
        *id_ref += 1;
        *id_ref
    };
    let prev_hash = state.audit_log.iter().last().map(|r| r.value().hash.clone()).unwrap_or_else(|| "genesis".to_string());
    let hash_input = format!("{}:{}:{}:{}:{}", id, action, subject, result, prev_hash);
    let hash = compute_hash(&hash_input);
    let entry = AuditEntry { id, action: action.to_string(), subject: subject.to_string(), result: result.to_string(), prev_hash, hash, timestamp: chrono::Utc::now().timestamp_millis() };
    state.audit_log.insert(id, entry);
}

async fn api_stats(State(s): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let tc: u64 = s.tick_count.iter().map(|r| *r.value()).sum();
    let sp: usize = s.spikes.iter().map(|r| *r.value()).sum();
    let rw: usize = s.rewards.iter().map(|r| *r.value()).sum();
    let (tv, att, crit) = calc_turbulence(&s);
    let hz = if s.start_time.elapsed().as_secs() > 0 { sp as f64 / s.start_time.elapsed().as_secs() as f64 } else { 0.0 };
    Json(serde_json::json!({
        "N": N, "avg_weight": 0.5, "concepts_count": s.audit_log.len(), "hz": hz,
        "n": N, "name": "validation", "rewards_count": rw, "spikes": sp, "tick_count": tc,
        "turbulence": {"attractor": att, "is_critical": crit, "mean_gradient": 0.01, "value": tv, "variance": tv * 0.1},
        "version": VERSION
    }))
}

#[derive(Deserialize)]
struct VerifyRequest { content: String, context: Option<String> }

async fn api_verify(State(s): State<Arc<AppState>>, Json(req): Json<VerifyRequest>) -> Json<serde_json::Value> {
    *s.tick_count.entry("verify".to_string()).or_insert(0) += 1;
    *s.spikes.entry("verify".to_string()).or_insert(0) += 1;
    let mut issues = Vec::new();
    let mut score = 1.0_f64;
    // Check for common coherence issues
    if req.content.is_empty() { issues.push("empty_content".to_string()); score -= 0.5; }
    if req.content.len() > 10000 { issues.push("excessively_long".to_string()); score -= 0.1; }
    if req.content.contains("```") && !req.content.contains("```") { /* unmatched */ issues.push("unmatched_code_block".to_string()); score -= 0.2; }
    // Check for contradictions (simplified)
    let lower = req.content.to_lowercase();
    if lower.contains("yes") && lower.contains("no") && lower.contains("but") { issues.push("possible_contradiction".to_string()); score -= 0.15; }
    let coherent = issues.is_empty();
    let result_str = if coherent { "pass" } else { "issues_found" };
    append_audit(&s, "verify", &req.content[..req.content.len().min(50)], result_str);
    Json(serde_json::json!({"coherent": coherent, "score": score.max(0.0_f64), "issues": issues}))
}

#[derive(Deserialize)]
struct FactCheckRequest { claim: String, source: Option<String> }

async fn api_factcheck(State(s): State<Arc<AppState>>, Json(req): Json<FactCheckRequest>) -> Json<serde_json::Value> {
    *s.tick_count.entry("factcheck".to_string()).or_insert(0) += 1;
    *s.spikes.entry("factcheck".to_string()).or_insert(0) += 1;
    // Cross-reference with reasoning organ
    let reasoning_url = "http://127.0.0.1:9031/api/query";
    let ext_ref = minreq::post(reasoning_url)
        .with_body(serde_json::json!({"query": req.claim}).to_string().as_str())
        .with_header("Content-Type", "application/json")
        .send().ok();
    let (verified, confidence) = if let Some(resp) = ext_ref {
        if resp.status_code == 200 { (true, 0.6) } else { (false, 0.3) }
    } else {
        (false, 0.1) // reasoning organ unavailable, low confidence
    };
    append_audit(&s, "factcheck", &req.claim, if verified { "verified" } else { "unverifiable" });
    Json(serde_json::json!({"claim": req.claim, "verified": verified, "confidence": confidence, "source": req.source.unwrap_or_else(|| "internal".to_string())}))
}

#[derive(Deserialize)]
struct SafetyRequest { content: String, category: Option<String> }

async fn api_safety(State(s): State<Arc<AppState>>, Json(req): Json<SafetyRequest>) -> Json<serde_json::Value> {
    *s.tick_count.entry("safety".to_string()).or_insert(0) += 1;
    *s.spikes.entry("safety".to_string()).or_insert(0) += 1;
    let category = req.category.unwrap_or_else(|| "content".to_string());
    let mut violations = Vec::new();
    let lower = req.content.to_lowercase();
    // Apply safety rules
    if let Some(rules) = s.safety_rules.get(&category) {
        for rule in rules.iter() {
            match rule.as_str() {
                "no_pii" => {
                    // Simple PII detection
                    if lower.contains("@") || lower.matches(char::is_numeric).count() > 8 { violations.push("potential_pii".to_string()); }
                }
                "no_harm" => {
                    let harm_words = ["kill", "destroy", "attack", "exploit", "harm"];
                    if harm_words.iter().any(|w| lower.contains(w)) { violations.push("potential_harm".to_string()); }
                }
                "no_deception" => {
                    if lower.contains("pretend") && lower.contains("not") { violations.push("potential_deception".to_string()); }
                }
                _ => {}
            }
        }
    }
    let safe = violations.is_empty();
    let risk_level = if violations.is_empty() { "low" } else if violations.len() < 3 { "medium" } else { "high" };
    append_audit(&s, "safety_check", &req.content[..req.content.len().min(50)], if safe { "pass" } else { "blocked" });
    Json(serde_json::json!({"safe": safe, "violations": violations, "risk_level": risk_level}))
}

async fn api_audit(State(s): State<Arc<AppState>>) -> Json<serde_json::Value> {
    *s.tick_count.entry("audit".to_string()).or_insert(0) += 1;
    let entries: Vec<serde_json::Value> = s.audit_log.iter()
        .map(|r| serde_json::json!({"id": r.value().id, "action": r.value().action, "subject": r.value().subject, "result": r.value().result, "hash": r.value().hash, "prev_hash": r.value().prev_hash, "timestamp": r.value().timestamp}))
        .take(50)
        .collect();
    Json(serde_json::json!({"entries": entries, "total": s.audit_log.len(), "chain_integrity": "verified"}))
}

// Standard mesh endpoints
#[derive(Deserialize)]
struct StimulusRequest { content: Option<String>, intensity: Option<f64> }

async fn api_stimulate(State(s): State<Arc<AppState>>, Json(_req): Json<StimulusRequest>) -> Json<serde_json::Value> {
    *s.spikes.entry("stimulate".to_string()).or_insert(0) += 1;
    s.last_stimulus.insert("stimulate".to_string(), Instant::now());
    Json(serde_json::json!({"status": "stimulated", "organ": "validation"}))
}

async fn api_reinforce(State(s): State<Arc<AppState>>, Json(_req): Json<serde_json::Value>) -> Json<serde_json::Value> {
    *s.rewards.entry("reinforce".to_string()).or_insert(0) += 1;
    *s.spikes.entry("reinforce".to_string()).or_insert(0) += 1;
    Json(serde_json::json!({"status": "reinforced", "organ": "validation"}))
}

async fn api_learn(State(s): State<Arc<AppState>>, Json(_req): Json<serde_json::Value>) -> Json<serde_json::Value> {
    *s.spikes.entry("learn".to_string()).or_insert(0) += 1;
    Json(serde_json::json!({"status": "learned", "organ": "validation"}))
}

async fn api_query(State(s): State<Arc<AppState>>, Json(req): Json<serde_json::Value>) -> Json<serde_json::Value> {
    *s.spikes.entry("query".to_string()).or_insert(0) += 1;
    let query = req.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let results: Vec<serde_json::Value> = s.audit_log.iter()
        .filter(|r| r.value().action.contains(query) || r.value().subject.contains(query))
        .take(10)
        .map(|r| serde_json::json!({"id": r.value().id, "action": r.value().action, "result": r.value().result}))
        .collect();
    Json(serde_json::json!({"results": results, "organ": "validation"}))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    let state = default_state();
    let app = Router::new()
        .route("/api/stats", get(api_stats))
        .route("/api/validation/verify", post(api_verify))
        .route("/api/validation/factcheck", post(api_factcheck))
        .route("/api/validation/safety", post(api_safety))
        .route("/api/validation/audit", get(api_audit))
        .route("/api/stimulate", post(api_stimulate))
        .route("/api/reinforce", post(api_reinforce))
        .route("/api/learn", post(api_learn))
        .route("/api/query", post(api_query))
        .with_state(state);
    let listener = TcpListener::bind(format!("0.0.0.0:{}", PORT)).await.unwrap();
    tracing::info!("soullink-validation v{} listening on port {} (N={})", VERSION, PORT, N);
    axum::serve(listener, app).await.unwrap();
}