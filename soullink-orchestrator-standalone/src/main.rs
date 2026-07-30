//! SoulLink Orchestrateur v3 — Rust Native
//!
//! NOTE: les structs d'API mesh (QueryRequest, BrainStats...) sont
//! l'échafaudage du protocole v3, pas encore toutes câblées — d'où le
//! allow(dead_code) global en attendant.
//! Remplace brain_orchestrator.py
//! Architecture: axum + tokio + dashmap + reqwest
//!
//! Nouveautés vs Python:
//!   - Routing turbulence-aware (préfère les cerveaux en StableOrbit)
//!   - Appels parallèles vrais (tokio::join_all)
//!   - Registry lock-free (DashMap)
//!   - Métriques Prometheus (/metrics)
//!   - Auto-spawn via Command::new

#![allow(dead_code)]

use axum::{
    routing::{get, post},
    Router,
};
use tower_http::trace::TraceLayer;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod auth;
mod models;
mod routes;
mod state;
mod utils;

use state::AppState;
use utils::helpers::parse_args;

/// Environment variable holding a comma-separated CORS origin allowlist.
///
/// Unset or blank permits no cross-origin browser access at all
/// (INV-NET-4). Each service names its own variable: they deploy
/// separately and have no reason to share an origin list.
const CORS_ALLOWLIST_VAR: &str = "SOULLINK_ORCHESTRATOR_CORS_ORIGINS";

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

    // MED-013-B: authentication. `/api/mesh/spawn` turns a request into a
    // process, so this listener refuses to start unauthenticated in
    // production rather than serving an open spawn route.
    let gate = auth::auth_from_env();
    if let Err(e) = gate.enforce_startup(auth::SERVICE, auth::AUTH_VARS) {
        eprintln!("FATAL: {e}");
        std::process::exit(1);
    }

    let app = build_router(state, gate);

    // Démarrage du serveur
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .expect("Failed to bind");

    info!("✅ Listening on 0.0.0.0:{}", port);

    axum::serve(listener, app).await.expect("Server error");
}

/// Build the HTTP surface.
///
/// Split out of `main` so the authentication layer can be exercised against
/// the real router in-process. A test that reimplements the wiring proves the
/// test's wiring works, not the service's.
fn build_router(state: AppState, gate: soul_auth::TokenAuth) -> Router {
    Router::new()
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
        // MED-013-B: every route above requires a bearer token.
        //
        // `route_layer`, not `layer`: an unrouted path answers 404 rather than
        // 401, so a prober learns nothing about which paths exist.
        //
        // Applied to the whole surface rather than to `/api/mesh/spawn` alone.
        // Spawn is the route that runs code, but `/api/mesh/brains` and
        // `/metrics` describe what this host is running and on which ports —
        // reconnaissance for the one route that matters. A Prometheus scraper
        // authenticates with `bearer_token_file`.
        .route_layer(axum::middleware::from_fn_with_state(
            gate.clone(),
            auth::require_auth,
        ))
        // Middlewares
        .layer(TraceLayer::new_for_http())
        // INV-NET-4: fail closed. `/api/mesh/spawn` is a POST route that
        // starts work, so a permissive header here was a browser-reachable
        // path to it.
        .layer(soul_cors::CorsPolicy::from_env(CORS_ALLOWLIST_VAR).read_write_layer())
}

#[cfg(test)]
mod auth_wiring_tests {
    //! MED-013-B: the authentication layer is exercised against the router
    //! `main` actually builds, via `build_router`.

    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn router_with(tokens: &[(&str, &str)]) -> Router {
        build_router(
            AppState::new("/tmp/soullink-test-brains"),
            soul_auth::TokenAuth::from_pairs(tokens.iter().copied()),
        )
    }

    async fn status_of(router: Router, req: Request<Body>) -> StatusCode {
        router
            .oneshot(req)
            .await
            .expect("router responded")
            .status()
    }

    /// The route that spawns a process refuses an anonymous caller.
    ///
    /// This is the finding: three defects in this route were fixed while it
    /// stayed reachable without credentials.
    #[tokio::test]
    async fn spawn_rejects_a_request_with_no_token() {
        let req = Request::post("/api/mesh/spawn")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"domain":"science"}"#))
            .unwrap();

        assert_eq!(
            status_of(router_with(&[("ops", "secret")]), req).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn spawn_rejects_a_wrong_token() {
        let req = Request::post("/api/mesh/spawn")
            .header("authorization", "Bearer not-the-token")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"domain":"science"}"#))
            .unwrap();

        assert_eq!(
            status_of(router_with(&[("ops", "secret")]), req).await,
            StatusCode::UNAUTHORIZED
        );
    }

    /// A valid token gets past the auth layer.
    ///
    /// Asserted as "not 401" rather than a specific success code: this test is
    /// about the gate, and pinning the handler's own status would make it fail
    /// for reasons that have nothing to do with authentication.
    #[tokio::test]
    async fn a_valid_token_passes_the_gate() {
        let req = Request::get("/api/mesh/status")
            .header("authorization", "Bearer secret")
            .body(Body::empty())
            .unwrap();

        assert_ne!(
            status_of(router_with(&[("ops", "secret")]), req).await,
            StatusCode::UNAUTHORIZED
        );
    }

    /// Read routes are covered too, not just spawn.
    ///
    /// `/api/mesh/brains` and `/metrics` report which brains run on which
    /// ports — reconnaissance for the route that matters.
    #[tokio::test]
    async fn reconnaissance_routes_are_also_gated() {
        for path in ["/api/mesh/brains", "/metrics", "/api/mesh/status"] {
            let req = Request::get(path).body(Body::empty()).unwrap();
            assert_eq!(
                status_of(router_with(&[("ops", "secret")]), req).await,
                StatusCode::UNAUTHORIZED,
                "{path} answered without a token"
            );
        }
    }

    /// An unrouted path 404s rather than 401s.
    ///
    /// Pins the `route_layer`/`layer` choice: `layer` would answer 401 for
    /// every path, telling a prober the service is listening and gated.
    #[tokio::test]
    async fn an_unknown_path_is_not_found_rather_than_unauthorized() {
        let req = Request::get("/no/such/route").body(Body::empty()).unwrap();
        assert_eq!(
            status_of(router_with(&[("ops", "secret")]), req).await,
            StatusCode::NOT_FOUND
        );
    }

    /// With no credentials configured, nothing authenticates.
    ///
    /// A service started with an empty token list in development is open to
    /// no one rather than open to everyone — `enforce_startup` warns, and the
    /// gate still rejects. Anything else would make the dev path a way to
    /// serve the spawn route anonymously.
    #[tokio::test]
    async fn an_unconfigured_gate_rejects_rather_than_admits() {
        let req = Request::post("/api/mesh/spawn")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"domain":"science"}"#))
            .unwrap();

        assert_eq!(
            status_of(router_with(&[]), req).await,
            StatusCode::UNAUTHORIZED
        );
    }
}
