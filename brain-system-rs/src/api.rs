use crate::{BrainState, neuron};
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;
use tokio::sync::RwLock;

pub type SharedState = Arc<RwLock<BrainState>>;

pub fn routes(state: SharedState) -> Router {
    Router::new()
        .route("/status", get(get_status))
        .route("/stimulus", post(post_stimulus))
        .route("/reset", post(reset_brain))
        .with_state(state)
}

async fn get_status(State(brain): State<SharedState>) -> Json<serde_json::Value> {
    let brain = brain.read().await;
    Json(serde_json::json!({
        "total_neurons": brain.total_neurons,
        "growth_events": brain.growth_events,
        "total_spikes": brain.total_spikes,
        "neurons": brain.neurons.len(),
        "synapses": brain.synapses.len(),
    }))
}

#[derive(serde::Deserialize)]
struct StimulusPayload {
    text: String,
    intensity: Option<f64>,
}

async fn post_stimulus(
    State(brain): State<SharedState>,
    Json(payload): Json<StimulusPayload>,
) -> Json<serde_json::Value> {
    let intensity = payload.intensity.unwrap_or(1.0);
    let count = (intensity * 10.0) as usize;
    let mut brain = brain.write().await;

    let modules = neuron::default_modules();
    let mod_idx = rand::random::<usize>() % modules.len();
    let m = &modules[mod_idx];

    let mut new_ids = Vec::new();
    for _ in 0..count.min(10) {
        let id = brain.total_neurons;
        brain.total_neurons += 1;
        let (x, y, z) = neuron::random_pos(m.pos, 60.0);
        brain.neurons.push(neuron::Neuron::new(id, &m.name, x, y, z));
        new_ids.push(id);
    }
    brain.growth_events += 1;

    // Create Hebbian connections
    let n = brain.neurons.len();
    if n > 1 {
        for &src in &new_ids {
            for _ in 0..3 {
                let target = rand::random::<usize>() % n;
                if target as u64 != src {
                    brain.synapses.push(neuron::Synapse::new(src, target as u64, rand::random::<f64>() * 0.5));
                }
            }
        }
    }

    Json(serde_json::json!({
        "created": count.min(10),
        "module": m.name,
        "total_neurons": brain.total_neurons,
        "total_synapses": brain.synapses.len(),
    }))
}

async fn reset_brain(State(brain): State<SharedState>) -> Json<serde_json::Value> {
    let mut brain = brain.write().await;
    brain.neurons.clear();
    brain.synapses.clear();
    brain.total_neurons = 0;
    brain.growth_events = 0;
    brain.total_spikes = 0;
    Json(serde_json::json!({"status": "reset"}))
}