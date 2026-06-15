//! Route: Turbulence

use crate::models::types::attractor_score;
use crate::state::AppState;
use crate::utils::helpers::now_ts;
use axum::{extract::State, Json};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Route GET /api/mesh/turbulence
pub async fn route_turbulence(State(state): State<AppState>) -> Json<Value> {
    let results = state.call_all_parallel("/api/turbulence", None, None).await;
    let mut mesh_turb = json!({});
    let mut critical_brains: Vec<String> = vec![];
    let mut attractor_counts: HashMap<String, u32> = HashMap::new();

    for (key, r) in &results {
        if r.get("error").is_none() {
            let att = r
                .get("attractor")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string();
            let val = r.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let crit = r
                .get("is_critical")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if crit {
                critical_brains.push(key.clone());
            }
            *attractor_counts.entry(att.clone()).or_insert(0) += 1;

            mesh_turb[key] = json!({
                "value": val,
                "critical": crit,
                "attractor": att,
            });
        }
    }

    // Trouver le cerveau le plus stable
    let best_brain = results
        .iter()
        .filter(|(_, r)| r.get("error").is_none())
        .max_by(|(_, a), (_, b)| {
            let sa = attractor_score(a.get("attractor").and_then(|v| v.as_str()).unwrap_or(""));
            let sb = attractor_score(b.get("attractor").and_then(|v| v.as_str()).unwrap_or(""));
            sa.partial_cmp(&sb).unwrap()
        })
        .map(|(k, _)| k.clone())
        .unwrap_or_default();

    Json(json!({
        "ok": true,
        "mesh": mesh_turb,
        "critical_brains": critical_brains,
        "attractor_distribution": attractor_counts,
        "most_stable_brain": best_brain,
        "ts": now_ts(),
    }))
}
