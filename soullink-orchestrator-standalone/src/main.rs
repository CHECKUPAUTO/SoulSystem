//! SoulLink Orchestrateur v3 — Rust Native
//!
//! Remplace brain_orchestrator.py
//! Architecture: axum + tokio + dashmap + reqwest
//!
//! Nouveautés vs Python:
//!   - Routing turbulence-aware (préfère les cerveaux en StableOrbit)
//!   - Appels parallèles vrais (tokio::join_all)
//!   - Registry lock-free (DashMap)
//!   - Métriques Prometheus (/metrics)
//!   - Auto-spawn via Command::new

use axum::{
    routing::{get, post},
    Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod models;
mod routes;
mod state;
mod utils;

use state::AppState;
use utils::helpers::parse_args;

#[tokio::main]
async fn main() {
    // Configuration du logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set subscriber");

    // Parse arguments
    let (port, brain_dir) = parse_args();

    // Création de l'état
    let state = AppState::new(&brain_dir);

    info!("🚀 SoulLink Orchestrateur v3 (Rust) — port {}", port);
    info!(
        "🧠 Cerveaux: {:?}",
        state
            .brains_iter()
            .map(|e| e.key().clone())
            .collect::<Vec<_>>()
    );

    // Construction du router
    let app = Router::new()
        // Routes d'index
        .route("/", get(routes::index::route_index))
        // Routes mesh
        .route("/api/mesh/status", get(routes::status::route_status))
        .route(
            "/api/mesh/turbulence",
            get(routes::turbulence::route_turbulence),
        )
        .route("/api/mesh/query", post(routes::query::route_query))
        .route("/api/mesh/think", post(routes::think::route_think))
        .route(
            "/api/mesh/reinforce",
            post(routes::reinforce::route_reinforce),
        )
        .route(
            "/api/mesh/stimulate",
            post(routes::stimulate::route_stimulate),
        )
        .route("/api/mesh/spawn", post(routes::spawn::route_spawn))
        .route("/api/mesh/brains", get(routes::brains::route_brains))
        // Metrics Prometheus
        .route("/metrics", get(routes::metrics::route_metrics))
        // State
        .with_state(state)
        // Middlewares
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    // Démarrage du serveur
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .expect("Failed to bind");

    info!("✅ Listening on 0.0.0.0:{}", port);

    axum::serve(listener, app).await.expect("Server error");
}
