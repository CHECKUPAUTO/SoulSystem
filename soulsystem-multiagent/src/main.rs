use anyhow::Result;
use soulsystem_multiagent::{run, MultiAgentConfig};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config: MultiAgentConfig = confy::load("soulsystem", "multiagent").unwrap_or_default();
    let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let _ = project_root;
    run(config).await
}
