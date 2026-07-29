#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    unused_mut,
    unused_unsafe,
    unreachable_pub,
    non_camel_case_types,
    non_snake_case,
    unused_comparisons
)]
mod api;
mod neuron;
mod persistence;
mod simulation;

use axum::Router;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Environment variable holding a comma-separated CORS origin allowlist.
///
/// Unset or blank permits no cross-origin browser access at all
/// (INV-NET-4). Each service names its own variable: they deploy
/// separately and have no reason to share an origin list.
const CORS_ALLOWLIST_VAR: &str = "BRAIN_SYSTEM_CORS_ORIGINS";

pub struct BrainState {
    pub neurons: Vec<neuron::Neuron>,
    pub synapses: Vec<neuron::Synapse>,
    pub total_neurons: u64,
    pub growth_events: u64,
    pub total_spikes: u64,
}

pub type SharedBrain = Arc<RwLock<BrainState>>;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let state = persistence::load_or_default().await;
    let brain: SharedBrain = Arc::new(RwLock::new(state));

    // Auto-save toutes les 30s
    let brain_clone = brain.clone();
    tokio::spawn(async move {
        simulation::auto_save_loop(brain_clone).await;
    });

    // Simulation loop (LIF ticks)
    let brain_clone2 = brain.clone();
    tokio::spawn(async move {
        simulation::simulation_loop(brain_clone2).await;
    });

    let app = Router::new()
        .nest("/api", api::routes(brain.clone()))
        // INV-NET-4: fail closed. `/api/stimulus` and `/api/reset` are POST
        // routes that mutate the running brain.
        .layer(soul_cors::CorsPolicy::from_env(CORS_ALLOWLIST_VAR).read_write_layer());

    let addr = "0.0.0.0:8084";
    tracing::info!("🧠 SoulLink Brain v8.5 (Rust) — listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
