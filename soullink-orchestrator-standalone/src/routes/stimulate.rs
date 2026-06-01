//! Route: Stimulate

use crate::state::AppState;
use axum::{extract::State, Json};
use serde_json::{json, Value};

/// Route POST /api/mesh/stimulate
pub async fn route_stimulate(State(state): State<AppState>, Json(body): Json<Value>) -> Json<Value> {
    let module = body
        .get("module")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let strength = body.get("strength").and_then(|v| v.as_f64()).unwrap_or(0.5);

    let selected = state.select_brains(&module);
    let results = state
        .call_all_parallel(
            "/api/stimulate",
            Some(json!({"module": module, "strength": strength})),
            Some(selected.clone()),
        )
        .await;

    let ok_count = results.values().filter(|r| r.get("error").is_none()).count();

    Json(json!({
        "ok": true,
        "module": module,
        "strength": strength,
        "stimulated": ok_count,
        "brains": selected,
    }))
}
