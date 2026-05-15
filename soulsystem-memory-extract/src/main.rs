use anyhow::Result;
use soulsystem_memory_extract::{run, MemoryExtractConfig};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).init();
    let config: MemoryExtractConfig = confy::load("soulsystem", "memory-extract").unwrap_or_default();
    run(config, &PathBuf::from("memory")).await
}
