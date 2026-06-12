mod agent;
mod audit;
mod batch_embed;
mod concept;
mod concept_graph;
mod config;
mod crawler;
mod evolution;
mod il;
pub mod math;
mod memory;
mod meta_language;
mod ollama;
mod organ_moe;
mod ranking;
mod sandbox;
mod snapshot;

use config::Config;
use evolution::EvolutionSystem;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .with_thread_ids(false)
        .compact()
        .init();

    println!(r#"
    ╔═══════════════════════════════════════════════╗
    ║                                               ║
    ║      ██████  ██████  ███████ ███    ██         ║
    ║     ██    ██ ██   ██ ██      ████   ██         ║
    ║     ██    ██ ██████  █████   ██ ██  ██         ║
    ║     ██    ██ ██      ██      ██  ██ ██         ║
    ║      ██████  ██      ███████ ██   ████         ║
    ║                                               ║
    ║       ██████ ██       █████  ██     ██         ║
    ║      ██      ██      ██   ██ ██     ██         ║
    ║      ██      ██      ███████ ██  █  ██         ║
    ║      ██      ██      ██   ██ ██ ███ ██         ║
    ║       ██████ ███████ ██   ██  ███ ███          ║
    ║                                               ║
    ║   v0.3.0 — Évolution AUDITÉE                 ║
    ║   Batch GPU | MoE Organes | GNN Graph | Audit ║
    ╚═══════════════════════════════════════════════╝
    "#);

    let config = Config::from_env();
    config.display();

    let mut system = EvolutionSystem::initialize(config).await?;
    system.run_forever().await?;

    Ok(())
}
