use axum::{
    extract::State,
    response::IntoResponse,
    Json,
};

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

pub async fn autonomy_handler(State(state): State<crate::AppState>) -> impl IntoResponse {
    let brain = state.brain.read().await;

    let age_ticks = brain.stats.age_ticks.max(1) as f64;

    let meta_activity = clamp01(
        brain.meta_cortex.loop_stats.total_interventions as f64 / age_ticks * 1000.0
    );
    let self_modify_activity = clamp01(
        brain.self_modify_engine.status().cycles_run as f64 / 10.0
    );
    let evolution_activity = clamp01(
        brain.evolution.generation as f64 / 100.0
    );
    let mesh_activity = clamp01(
        brain.mesh.peers.len() as f64 / 5.0
    );

    let autonomy_score = 0.3 * meta_activity
        + 0.3 * self_modify_activity
        + 0.2 * evolution_activity
        + 0.2 * mesh_activity;

    let grade = if autonomy_score < 0.2 {
        "embryo"
    } else if autonomy_score < 0.4 {
        "larva"
    } else if autonomy_score < 0.6 {
        "pupa"
    } else if autonomy_score < 0.8 {
        "organism"
    } else {
        "conscious"
    };

    Json(serde_json::json!({
        "autonomy_score": autonomy_score,
        "grade": grade,
        "components": {
            "meta_activity": meta_activity,
            "self_modify_activity": self_modify_activity,
            "evolution_activity": evolution_activity,
            "mesh_activity": mesh_activity,
        }
    }))
}
