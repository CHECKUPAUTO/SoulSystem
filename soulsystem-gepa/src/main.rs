use anyhow::Result;
use soulsystem_gepa::{run, GepaConfig};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let config: GepaConfig = confy::load("soulsystem", "gepa").unwrap_or_default();
    let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    run(config, &project_root)
}
