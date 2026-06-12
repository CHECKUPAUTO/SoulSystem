#![forbid(unsafe_code)]
#![warn(clippy::pedantic, clippy::nursery)]
#![allow(clippy::module_name_repetitions)]

pub mod api;
pub use api::{build_router, AppState};

use std::sync::Arc;
use tracing::info;

use avid_core::config::Config;
use avid_core::llm::LlmClient;
use avid_core::memory::MemoryStore;

async fn launch_headless() -> Option<Arc<avid_scout::headless::HeadlessBrowser>> {
    info!("Initializing headless Chromium browser...");
    match avid_scout::headless::HeadlessBrowser::launch().await {
        Ok(browser) => {
            info!("Headless browser ready");
            Some(Arc::new(browser))
        }
        Err(e) => {
            handle_launch_error(&e.into());
            None
        }
    }
}

fn handle_launch_error(e: &anyhow::Error) {
    tracing::warn!(
        "Headless browser launch failed (AVID_HEADLESS=1 but chromium not available?): {e}"
    );
    info!("Continuing without headless browser — call /v1/headless/navigate will return 503");
}

async fn init_headless() -> Option<Arc<avid_scout::headless::HeadlessBrowser>> {
    if std::env::var("AVID_HEADLESS").is_ok_and(|v| v == "1" || v.to_lowercase() == "true") {
        launch_headless().await
    } else {
        info!("Headless browser disabled. Set AVID_HEADLESS=1 to enable");
        None
    }
}

/// Runs the AVID API server.
///
/// # Errors
/// Returns an error if config is invalid or the server fails to start.
pub async fn run() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    let config = Arc::new(config);

    let llm =
        Arc::new(LlmClient::new(config.ollama_url.clone(), config.ollama_model.clone()).await?);

    let memory = Arc::new(MemoryStore::new(&config.soullink_db_path(), 4)?);

    let headless = init_headless().await;

    let state = AppState {
        config,
        memory,
        llm,
        headless,
    };

    start_server(state).await
}

async fn start_server(state: AppState) -> anyhow::Result<()> {
    let app = api::build_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
    info!("AVID server listening on 127.0.0.1:8080");
    axum::serve(listener, app).await?;
    Ok(())
}
