//! SoulLink Orchestrator - Utility Helpers

use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Timestamp actuel en secondes
pub fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Fusionne les concepts de plusieurs cerveaux
pub fn merge_concepts(results: &HashMap<String, Value>) -> Vec<Value> {
    let mut all: HashMap<String, (f64, f64, String)> = HashMap::new(); // concept → (score, mastery, brain)

    for (brain, r) in results {
        if let Some(concepts) = r.get("top_concepts").and_then(|v| v.as_array()) {
            for c in concepts {
                let concept = c
                    .get("concept")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let mastery = c.get("mastery").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let score = c.get("score").and_then(|v| v.as_f64()).unwrap_or(mastery);

                if concept.is_empty() {
                    continue;
                }

                let entry = all.entry(concept).or_insert((0.0, 0.0, brain.clone()));
                if score > entry.0 {
                    *entry = (score, mastery, brain.clone());
                }
            }
        }
    }

    let mut sorted: Vec<_> = all.into_iter().collect();
    sorted.sort_by(|a, b| b.1 .0.partial_cmp(&a.1 .0).unwrap());

    sorted
        .iter()
        .take(20)
        .map(|(concept, (score, mastery, brain))| {
            json!({
                "concept": concept,
                "score": score,
                "mastery": mastery,
                "source_brain": brain,
            })
        })
        .collect()
}

/// Calcule la moyenne de mastery
pub fn calculate_avg_mastery(concepts: &[Value]) -> f64 {
    if concepts.is_empty() {
        return 0.0;
    }

    let sum: f64 = concepts
        .iter()
        .filter_map(|c| c.get("mastery").and_then(|v| v.as_f64()))
        .sum();

    sum / concepts.len() as f64
}

/// Formate les métriques Prometheus
pub fn format_metrics(queries: u64, spawns: u64, brains: usize) -> String {
    format!(
        "# SoulLink Orchestrateur v3 Metrics\n\
         # HELP soullink_queries_total Total number of queries processed\n\
         # TYPE soullink_queries_total counter\n\
         soullink_queries_total {}\n\
         # HELP soullink_spawns_total Total number of brain spawns\n\
         # TYPE soullink_spawns_total counter\n\
         soullink_spawns_total {}\n\
         # HELP soullink_brains_registered Number of registered brains\n\
         # TYPE soullink_brains_registered gauge\n\
         soullink_brains_registered {}\n",
        queries, spawns, brains
    )
}

/// Trouve le prochain port disponible
pub fn find_next_port(used_ports: &[u16], start: u16) -> u16 {
    let mut port = start;
    while used_ports.contains(&port) {
        port += 1;
    }
    port
}

/// Parse les arguments de ligne de commande
pub fn parse_args() -> (u16, String) {
    let args: Vec<String> = std::env::args().collect();

    let port = args
        .get(1)
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(9020);

    let brain_dir = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| "/mnt/nvme/soullink_brain".to_string());

    (port, brain_dir)
}
