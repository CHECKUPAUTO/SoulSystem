//! Route: Reinforce

use crate::state::AppState;
use axum::{extract::State, Json};
use serde_json::{json, Value};

/// Route POST /api/mesh/reinforce
pub async fn route_reinforce(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let concept = body
        .get("concept")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let delta = body.get("delta").and_then(|v| v.as_f64()).unwrap_or(0.05);
    let brains: Option<Vec<String>> = body.get("brains").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect()
    });

    if concept.is_empty() {
        return Json(json!({"ok": false, "error": "concept requis"}));
    }

    let selected = brains.unwrap_or_else(|| state.select_brains(&concept));
    let results = state
        .call_all_parallel(
            "/api/reinforce",
            Some(json!({"concept": concept, "delta": delta})),
            Some(selected.clone()),
        )
        .await;

    let updated: Vec<_> = results
        .iter()
        .filter(|(_, r)| r.get("error").is_none())
        .map(|(k, r)| json!({"brain": k, "mastery": r.get("mastery")}))
        .collect();

    Json(json!({
        "ok": true,
        "concept": concept,
        "delta": delta,
        "updated": updated.len(),
        "results": updated,
    }))
}
