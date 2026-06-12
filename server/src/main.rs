use sqlx::postgres::PgPoolOptions;
use std::env;
use server::{AppState, router, llm::LlmClient};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,sqlx=warn".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&database_url)
        .await?;

    let llm_provider = env::var("LLM_PROVIDER").unwrap_or_else(|_| "ollama".into());
    let llm_url = env::var("OLLAMA_URL").unwrap_or_else(|_| "http://localhost:11434".into());
    let anthropic_key = env::var("ANTHROPIC_API_KEY").ok();

    let llm_client = LlmClient::new(llm_provider, llm_url, anthropic_key);
    let state = AppState::new(pool, llm_client);

    let app = router::create_router(state);

    let addr = env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("listening on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
