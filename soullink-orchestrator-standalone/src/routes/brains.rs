//! Route: Brains

use crate::state::AppState;
use axum::{extract::State, Json};
use serde_json::{json, Value};

/// Route GET /api/mesh/brains
pub async fn route_brains(State(state): State<AppState>) -> Json<Value> {
    let brains: std::collections::HashMap<String, Value> = state
        .brains_iter()
        .map(|entry| {
            let k = entry.key().clone();
            let v = entry.value().clone();
            (
                k,
                json!({
                    "port": v.port,
                    "speciality": v.speciality,
                    "version": v.version,
                    "spawned_by": v.spawned_by,
                }),
            )
        })
        .collect();

    Json(json!({
        "ok": true,
        "brains": brains,
        "total": brains.len(),
    }))
}
