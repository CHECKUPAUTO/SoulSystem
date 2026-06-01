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
use tower_http::cors::CorsLayer;

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
    let app = build_router(state).layer(CorsLayer::permissive());

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
