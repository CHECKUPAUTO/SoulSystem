//! Route: Query

use crate::models::types::attractor_score;
use crate::state::AppState;
use crate::utils::helpers::{calculate_avg_mastery, merge_concepts, now_ts};
use axum::{extract::State, Json};
use serde_json::{json, Value};

/// Route POST /api/mesh/query
pub async fn route_query(State(state): State<AppState>, Json(body): Json<Value>) -> Json<Value> {
    let question = body
        .get("question")
        .or_else(|| body.get("q"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if question.is_empty() {
        return Json(json!({"ok": false, "error": "question requise"}));
    }

    state.increment_query_count();

    // Sélection des cerveaux
    let mut selected = state.select_brains(&question);

    // Pondération par turbulence
    let turb_results = state
        .call_all_parallel("/api/turbulence", None, Some(selected.clone()))
        .await;

    // Re-trier par stabilité
    selected.sort_by(|a, b| {
        let sa = turb_results
            .get(a)
            .and_then(|r| r.get("attractor"))
            .and_then(|v| v.as_str())
            .map(attractor_score)
            .unwrap_or(0.5);
        let sb = turb_results
            .get(b)
            .and_then(|r| r.get("attractor"))
            .and_then(|v| v.as_str())
            .map(attractor_score)
            .unwrap_or(0.5);
        sb.partial_cmp(&sa).unwrap()
    });

    // Appels parallèles
    let results = state
        .call_all_parallel(
            "/api/query",
            Some(json!({"question": question})),
            Some(selected.clone()),
        )
        .await;

    let merged = merge_concepts(&results);
    let avg_mastery = calculate_avg_mastery(&merged);

    Json(json!({
        "ok": true,
        "question": question,
        "brains_queried": selected,
        "concepts_found": merged.len(),
        "top_concepts": merged,
        "avg_mastery": avg_mastery,
        "routing": "turbulence-aware",
        "ts": now_ts(),
    }))
}
