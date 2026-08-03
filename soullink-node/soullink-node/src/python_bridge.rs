//! Python Bridge — Bidirectional bridge between Python SoulSystem 2026 and Rust SoulLink.
//!
//! Correction 4: Exposes a unified HTTP endpoint so Python modules
//! (soulsystem_core.py, darwin_engine.py, mem0_graph.py, etc.) can
//! query and control the Rust brain instead of directly hitting Ollama.

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

/// Requête provenant du Python (soulsystem_core.py, darwin_engine.py, etc.)
#[derive(Debug, Deserialize)]
pub struct PythonRequest {
    pub module: String, // "darwin_engine", "mem0_graph", etc.
    pub action: String, // "evolve", "query_memory", "compress_sop"
    pub payload: serde_json::Value,
}

/// Réponse renvoyée au Python
#[derive(Debug, Serialize)]
pub struct PythonResponse {
    pub success: bool,
    pub data: serde_json::Value,
    pub metrics: Option<BridgeMetrics>,
}

#[derive(Debug, Serialize)]
pub struct BridgeMetrics {
    pub latency_ms: f64,
    pub brain_tick: u64,
    pub energy_pool: f32,
}

/// Endpoint du bridge Python — exposé comme route HTTP.
/// Traite les requêtes des modules Python et les forward au brain.
pub async fn handle_python_bridge(
    State(state): State<crate::AppState>,
    Json(req): Json<PythonRequest>,
) -> Json<PythonResponse> {
    let start = std::time::Instant::now();
    let mut brain = state.brain.write().await;

    let result = match (req.module.as_str(), req.action.as_str()) {
        // ── Mem0 Graph — requête mémoire ──
        ("mem0_graph", "query") => {
            let query = req
                .payload
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            // Search learned topics as memory
            let results: Vec<&String> = brain
                .learned_topics
                .iter()
                .filter(|t| t.contains(query))
                .take(5)
                .collect();
            PythonResponse {
                success: true,
                data: serde_json::json!({"results": results}),
                metrics: None,
            }
        }

        // ── Darwin Engine — évolution ──
        ("darwin_engine", "evolve") => {
            let generations = req
                .payload
                .get("generations")
                .and_then(|v| v.as_u64())
                .unwrap_or(1) as usize;
            let mut best_score = brain.evolution.best_fitness;
            for _ in 0..generations {
                let mut e = std::mem::take(&mut brain.evolution);
                e.step(&mut brain);
                best_score = e.best_fitness;
                brain.evolution = e;
            }
            PythonResponse {
                success: true,
                data: serde_json::json!({"best_score": best_score}),
                metrics: None,
            }
        }

        // ── SOP Compressor — compression ──
        ("sop_compressor", "compress") => {
            let text = req
                .payload
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            // Simple SOP compression: extract key tokens
            let compressed: String = text
                .split_whitespace()
                .take(32)
                .collect::<Vec<_>>()
                .join(" ");
            PythonResponse {
                success: true,
                data: serde_json::json!({"compressed": compressed}),
                metrics: None,
            }
        }

        // ── Soullink Stream — état ──
        ("soullink_stream", "status") => {
            PythonResponse {
                success: true,
                data: serde_json::json!({
                    "node_name": brain.node_name,
                    "node_port": brain.node_port,
                    "neurons": brain.neurons.len(),
                    "synapses": brain.synapses.len(),
                    "generation": brain.evolution.generation,
                    "fitness": brain.evolution.best_fitness,
                    "energy_pool": brain.energy_pool,
                    "stress_mode": format!("{:?}", brain.stress_mode),
                }),
                metrics: None,
            }
        }

        // ── Unknown ──
        _ => PythonResponse {
            success: false,
            data: serde_json::json!({
                "error": format!(
                    "unknown module/action: {}/{}",
                    req.module, req.action
                )
            }),
            metrics: None,
        },
    };

    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    Json(PythonResponse {
        metrics: Some(BridgeMetrics {
            latency_ms: elapsed,
            brain_tick: brain.stats.age_ticks,
            energy_pool: brain.energy_pool,
        }),
        ..result
    })
}
