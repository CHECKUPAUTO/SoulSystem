//! Route: Status

use crate::models::types::{attractor_score, MeshStatus};
use crate::state::AppState;
use crate::utils::helpers::now_ts;
use axum::{extract::State, Json};
use serde_json::{json, Value};

/// Route GET /api/mesh/status
pub async fn route_status(State(state): State<AppState>) -> Json<Value> {
    let results = state.call_all_parallel("/api/stats", None, None).await;
    let mut mesh = json!({});
    let mut total_n = 0u64;
    let mut online = 0u32;

    for (key, r) in &results {
        if r.get("error").is_some() {
            mesh[key] = json!({
                "state": "offline",
                "port": state.get_brain_port(key).unwrap_or(0)
            });
        } else {
            let n = r.get("N").and_then(|v| v.as_u64()).unwrap_or(0);
            let hz = r.get("hz").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let turb = r.get("turbulence");
            let att = turb
                .and_then(|t| t.get("attractor"))
                .and_then(|v| v.as_str())
                .unwrap_or("?");

            total_n += n;
            online += 1;

            mesh[key] = json!({
                "state": "online",
                "N": n,
                "hz": hz,
                "attractor": att,
                "port": state.get_brain_port(key).unwrap_or(0),
                "version": r.get("version").and_then(|v| v.as_str()).unwrap_or("?"),
            });
        }
    }

    Json(json!({
        "ok": true,
        "mesh": mesh,
        "total_N": total_n,
        "online": online,
        "total_brains": state.brains_count(),
        "ts": now_ts(),
    }))
}
