//! SoulLink Autonomy — The heartbeat of the neural mesh.
//!
//! Runs 3 loops:
//! 1. Pulse: 1Hz Hamiltonian evolution (Nesterov momentum)
//! 2. Dream: 10min random walk on MemoryGraph
//! 3. Afferent: Continuous sensor reading (GPU temp → turbulence)

use soullink_autonomy::afferent::AfferentNerve;
use soullink_autonomy::dream::{DreamConfig, DreamCycle};
use soullink_autonomy::node::NodeId;
use soullink_autonomy::pulse::AutonomyPulse;
use std::sync::Arc;
use tracing::info;

#[tokio::main(worker_threads = 4)]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    info!("🦞 SoulLink Autonomy v13.5 starting...");

    let orchestrator_url = std::env::var("ORCHESTRATOR_URL")
        .unwrap_or("http://127.0.0.1:9020".to_string());
    let pulse_interval_ms: u64 = std::env::var("PULSE_INTERVAL_MS")
        .unwrap_or("1000".to_string())
        .parse()
        .unwrap_or(1000);
    let dream_interval_secs: u64 = std::env::var("DREAM_INTERVAL_SECS")
        .unwrap_or("600".to_string())
        .parse()
        .unwrap_or(600);

    info!("Orchestrator: {}", orchestrator_url);
    info!("Pulse interval: {}ms", pulse_interval_ms);
    info!("Dream interval: {}s", dream_interval_secs);

    // Create components
    let afferent = Arc::new(AfferentNerve::new(&orchestrator_url));
    let pulse = Arc::new(AutonomyPulse::new(afferent));

    let dream_config = DreamConfig {
        cycle_interval_secs: dream_interval_secs,
        db_path: std::env::var("MEMORY_DB_PATH")
            .unwrap_or("/var/lib/soullink/memory".to_string()),
        ..Default::default()
    };
    let dream = Arc::new(DreamCycle::new(dream_config));

    // Spawn pulse loop (1Hz)
    let pulse_clone = pulse.clone();
    let pulse_handle = tokio::spawn(async move {
        pulse_clone.run(pulse_interval_ms).await;
    });

    // Spawn dream loop
    let dream_handle = tokio::spawn(async move {
        dream.run().await;
    });

    // Simple status API
    let pulse_for_api = pulse.clone();
    let app = axum::Router::new()
        .route("/api/autonomy/status", axum::routing::get({
            let pulse = pulse_for_api.clone();
            move || {
                let snap = pulse.snapshot();
                async move { axum::Json(snap) }
            }
        }));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:9046").await
        .expect("Failed to bind autonomy API on port 9046");
    info!("Autonomy API listening on :9046");

    axum::serve(listener, app)
        .await
        .expect("Autonomy API server failed");
}