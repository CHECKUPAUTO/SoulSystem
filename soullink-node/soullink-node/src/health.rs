use axum::{
    extract::State,
    response::IntoResponse,
    Json,
};

pub async fn health_handler(State(state): State<crate::AppState>) -> impl IntoResponse {
    let brain = state.brain.read().await;

    let status = if brain.stats.energy_avg > 0.01 {
        "healthy"
    } else {
        "degraded"
    };

    Json(serde_json::json!({
        "status": status,
        "uptime_ticks": brain.stats.age_ticks,
        "neurons": brain.neurons.len(),
        "energy_avg": brain.stats.energy_avg,
        "stress_mode": format!("{:?}", brain.stress_mode),
        "meta_cortex_phase": format!("{:?}", brain.meta_cortex.loop_stats.phase),
        "self_modify_cycles": brain.self_modify_engine.status().cycles_run,
        "evolution_generation": brain.evolution.generation,
    }))
}
