//! SoulLink Orchestrator - Core Types
//!
//! Defines all data structures for the neural mesh orchestration system.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration d'un cerveau dans le mesh
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainConfig {
    pub port: u16,
    pub url: String,
    pub speciality: Vec<String>,
    pub version: String,
    #[serde(default)]
    pub spawned_by: String,
}

impl BrainConfig {
    pub fn new(_name: &str, port: u16, speciality: Vec<&str>) -> Self {
        Self {
            port,
            url: format!("http://localhost:{}", port),
            speciality: speciality.iter().map(|s| s.to_string()).collect(),
            version: "v12".to_string(),
            spawned_by: "default".to_string(),
        }
    }
}

/// Statistiques d'un cerveau
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BrainStats {
    #[serde(rename = "N", default)]
    pub n: u32,
    #[serde(default)]
    pub hz: f64,
    #[serde(default)]
    pub spk: u64,
    #[serde(default)]
    pub gpu: bool,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub turbulence: Option<TurbulenceReport>,
}

/// Rapport de turbulence d'un cerveau
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TurbulenceReport {
    #[serde(default)]
    pub value: f64,
    #[serde(default)]
    pub is_critical: bool,
    #[serde(default)]
    pub attractor: String,
    #[serde(default)]
    pub mean_gradient: f64,
    #[serde(default)]
    pub variance: f64,
}

/// Résultat d'une requête
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    #[serde(default)]
    pub brain: String,
    #[serde(default)]
    pub concepts_found: usize,
    #[serde(default)]
    pub top_concepts: Vec<ConceptEntry>,
    #[serde(default)]
    pub avg_mastery: f64,
    #[serde(default)]
    pub engine: String,
}

/// Entrée conceptuelle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptEntry {
    pub concept: String,
    #[serde(default)]
    pub mastery: f64,
    #[serde(default)]
    pub score: f64,
    #[serde(default)]
    pub source_brain: String,
}

/// Requête de query
#[derive(Debug, Deserialize)]
pub struct QueryRequest {
    #[serde(alias = "q")]
    pub question: String,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub brains: Option<Vec<String>>,
}

/// Requête de renforcement
#[derive(Debug, Deserialize)]
pub struct ReinforceRequest {
    pub concept: String,
    #[serde(default = "default_delta")]
    pub delta: f64,
    #[serde(default)]
    pub brains: Option<Vec<String>>,
}

fn default_delta() -> f64 {
    0.05
}

/// Requête de stimulation
#[derive(Debug, Deserialize)]
pub struct StimulateRequest {
    pub module: String,
    #[serde(default = "default_strength")]
    pub strength: f64,
}

fn default_strength() -> f64 {
    0.5
}

/// Requête de spawn
#[derive(Debug, Deserialize)]
pub struct SpawnRequest {
    pub domain: String,
    #[serde(default)]
    pub speciality: Option<Vec<String>>,
}

/// Requête de think
#[derive(Debug, Deserialize)]
pub struct ThinkRequest {
    pub task: String,
    #[serde(default)]
    pub context: Option<String>,
}

/// Réponse standard API
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub ok: bool,
    #[serde(flatten)]
    pub data: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            ok: true,
            data,
            error: None,
        }
    }

    pub fn error(msg: impl Into<String>) -> ApiResponse<ErrorData> {
        ApiResponse {
            ok: false,
            data: ErrorData {
                message: msg.into(),
            },
            error: Some("error".to_string()),
        }
    }
}

/// Données d'erreur
#[derive(Debug, Serialize)]
pub struct ErrorData {
    pub message: String,
}

/// État du mesh
#[derive(Debug, Serialize)]
pub struct MeshStatus {
    pub mesh: HashMap<String, serde_json::Value>,
    pub total_n: u64,
    pub online: u32,
    pub total_brains: usize,
    pub ts: u64,
}

/// État de turbulence du mesh
#[derive(Debug, Serialize)]
pub struct MeshTurbulence {
    pub mesh: serde_json::Value,
    pub critical_brains: Vec<String>,
    pub attractor_distribution: HashMap<String, u32>,
    pub most_stable_brain: String,
    pub ts: u64,
}

/// Résultat de query
#[derive(Debug, Serialize)]
pub struct QueryResponse {
    pub question: String,
    pub brains_queried: Vec<String>,
    pub concepts_found: usize,
    pub top_concepts: Vec<serde_json::Value>,
    pub avg_mastery: f64,
    pub routing: String,
    pub ts: u64,
}

/// Résultat de think
#[derive(Debug, Serialize)]
pub struct ThinkResponse {
    pub task: String,
    pub brains_consulted: Vec<String>,
    pub concepts_found: usize,
    pub top_concepts: Vec<serde_json::Value>,
    pub ts: u64,
}

/// Résultat de renforcement
#[derive(Debug, Serialize)]
pub struct ReinforceResponse {
    pub concept: String,
    pub delta: f64,
    pub updated: usize,
    pub results: Vec<serde_json::Value>,
}

/// Résultat de stimulation
#[derive(Debug, Serialize)]
pub struct StimulateResponse {
    pub module: String,
    pub strength: f64,
    pub stimulated: usize,
    pub brains: Vec<String>,
}

/// Résultat de spawn
#[derive(Debug, Serialize)]
pub struct SpawnResponse {
    pub domain: String,
    pub port: u16,
    pub msg: String,
    pub check: String,
}

/// Liste des cerveaux
#[derive(Debug, Serialize)]
pub struct BrainsList {
    pub brains: HashMap<String, serde_json::Value>,
    pub total: usize,
}

/// Index de l'API
#[derive(Debug, Serialize)]
pub struct ApiIndex {
    pub name: String,
    pub version: String,
    pub engine: String,
    pub features: Vec<String>,
    pub endpoints: Vec<String>,
}

impl Default for ApiIndex {
    fn default() -> Self {
        Self {
            name: "SoulLink Orchestrateur v3 — Rust Native".to_string(),
            version: "3.0.0".to_string(),
            engine: "axum + tokio + dashmap".to_string(),
            features: vec![
                "turbulence-aware-routing".to_string(),
                "parallel-calls".to_string(),
                "auto-spawn".to_string(),
                "prometheus-metrics".to_string(),
                "lock-free-registry".to_string(),
            ],
            endpoints: vec![
                "GET  /".to_string(),
                "GET  /api/mesh/status".to_string(),
                "GET  /api/mesh/turbulence".to_string(),
                "POST /api/mesh/query".to_string(),
                "POST /api/mesh/think".to_string(),
                "POST /api/mesh/reinforce".to_string(),
                "POST /api/mesh/stimulate".to_string(),
                "POST /api/mesh/spawn".to_string(),
                "GET  /api/mesh/brains".to_string(),
                "GET  /metrics".to_string(),
            ],
        }
    }
}

/// Score d'attracteur pour le routing
pub fn attractor_score(attractor: &str) -> f64 {
    match attractor {
        "StableOrbit" => 1.0,
        "DeepBasin" => 0.6,
        "Transient" => 0.4,
        "Initializing" => 0.3,
        "StrangeAttractor" => 0.1,
        _ => 0.5,
    }
}
