//! TurboQuant Proxy Binary — Live KV cache compression HTTP server.
//!
//! Sits between clients and llama-server, adding 3-bit KV offload:
//!   Client → :8082 (this proxy) → :8081 (llama-server Q4_0 KV)
//!
//! When GPU KV cache fills up (>80%), old positions are compressed
//! from Q4_0 (4x) to TurboQuant 3-bit (6x total vs FP16).
//!
//! Endpoints:
//!   POST /v1/chat/completions  → proxy to llama-server with KV offload
//!   POST /v1/completions       → proxy to llama-server
//!   GET  /v1/models            → proxy to llama-server
//!   GET  /health               → health check
//!   GET  /stats                → compression + usage stats

use soullink_inference::turboquant::proxy::router::{build_router, AppState};
use soullink_inference::turboquant::proxy::server::{ProxyConfig, TurboQuantProxy};

/// Environment variable holding a comma-separated CORS origin allowlist.
///
/// Unset or blank permits no cross-origin browser access at all
/// (INV-NET-4). Each service names its own variable: they deploy
/// separately and have no reason to share an origin list.
const CORS_ALLOWLIST_VAR: &str = "TURBOQUANT_PROXY_CORS_ORIGINS";

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let config = ProxyConfig {
        listen_port: std::env::var("TURBOQUANT_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8082),
        llama_server_url: std::env::var("LLAMA_SERVER_URL")
            .unwrap_or_else(|_| "http://localhost:8081".into()),
        gpu_kv_capacity: std::env::var("GPU_KV_CAPACITY")
            .ok()
            .and_then(|c| c.parse().ok())
            .unwrap_or(4096),
        offload_threshold: std::env::var("OFFLOAD_THRESHOLD")
            .ok()
            .and_then(|t| t.parse().ok())
            .unwrap_or(0.8),
    };

    eprintln!("🧠 TurboQuant Proxy starting...");
    eprintln!("   Listen:        :{}", config.listen_port);
    eprintln!("   Backend:       {}", config.llama_server_url);
    eprintln!("   GPU capacity:  {} positions", config.gpu_kv_capacity);
    eprintln!(
        "   Offload at:    {}%",
        (config.offload_threshold * 100.0) as u32
    );
    eprintln!("   Compression:   6x vs FP16 (Q4_0 4x + TQ 3-bit)");

    let proxy = TurboQuantProxy::new(config);
    let listen_port = proxy.config().listen_port;

    let healthy = proxy.health_check().await;
    if healthy {
        eprintln!("✅ Backend llama-server Q4_0 KV healthy");
    } else {
        eprintln!("⚠️  Backend not responding — will retry on requests");
    }

    let state = AppState { proxy };
    // INV-NET-4: fail closed. This proxy forwards `/v1/chat/completions`
    // to a local llama-server; `permissive()` let any page the operator had
    // open drive it and read the replies.
    let app = build_router(state)
        .layer(soul_cors::CorsPolicy::from_env(CORS_ALLOWLIST_VAR).read_write_layer());

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", listen_port))
        .await
        .expect("Failed to bind port");

    eprintln!("🚀 TurboQuant Proxy listening on :{}", listen_port);
    eprintln!("   POST /v1/chat/completions  → proxy with KV offload");
    eprintln!("   POST /v1/completions       → proxy with KV offload");
    eprintln!("   GET  /v1/models            → proxy models");
    eprintln!("   GET  /health               → health check");
    eprintln!("   GET  /stats                 → compression stats");

    axum::serve(listener, app).await.expect("Server error");
}
