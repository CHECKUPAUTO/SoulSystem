use clawd::{ClawdContext, Settings, run_bot};
use std::sync::Arc;
use axum::{routing::get, Router};
use std::net::SocketAddr;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let bot_token = std::env::var("TELEGRAM_BOT_TOKEN")
        .expect("TELEGRAM_BOT_TOKEN must be set");

    let bus = Arc::new(bus::Bus::new(256));
    let settings = Settings {
        bot_token,
        avid_endpoint: "http://localhost:7878".to_string(),
    };

    let context = Arc::new(ClawdContext::new(settings, bus)?);

    // 1. Heartbeat HTTP endpoint on :9051/health
    let app = Router::new().route("/health", get(|| async { "OK" }));

    let addr = SocketAddr::from(([0, 0, 0, 0], 9051));
    info!("Heartbeat server listening on {}", addr);

    let context_bot = context.clone();
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });

    // 2. Start Telegram Bot
    info!("Starting clawd Telegram bot...");
    run_bot(context_bot).await?;

    Ok(())
}
