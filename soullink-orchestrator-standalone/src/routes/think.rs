//! Route: Think

use crate::state::AppState;
use crate::utils::helpers::{merge_concepts, now_ts};
use axum::{extract::State, Json};
use serde_json::{json, Value};

/// Route POST /api/mesh/think
pub async fn route_think(State(state): State<AppState>, Json(body): Json<Value>) -> Json<Value> {
    let task = body
        .get("task")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let context = body
        .get("context")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if task.is_empty() {
        return Json(json!({"ok": false, "error": "task requis"}));
    }

    let query = format!("{} {}", task, context);
    let selected = state.select_brains(&query);
    let results = state
        .call_all_parallel(
            "/api/query",
            Some(json!({"question": task})),
            Some(selected.clone()),
        )
        .await;

    let merged = merge_concepts(&results);

    Json(json!({
        "ok": true,
        "task": task,
        "brains_consulted": selected,
        "concepts_found": merged.len(),
        "top_concepts": &merged[..merged.len().min(10)],
        "ts": now_ts(),
    }))
}
