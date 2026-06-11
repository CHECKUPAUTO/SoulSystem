pub mod backup;
pub mod evolution;
pub mod fitness;

use anyhow::Result;
use tracing::info;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GepaConfig {
    pub threshold: f64,
    pub backup_enabled: bool,
    pub parallel: bool,
    pub max_generations: u32,
}

impl Default for GepaConfig {
    fn default() -> Self {
        Self {
            threshold: 0.75,
            backup_enabled: true,
            parallel: true,
            max_generations: 10,
        }
    }
}

pub fn run(config: GepaConfig, project_root: &std::path::Path) -> Result<()> {
    info!("Starting GEPA evolution with config: {:?}", config);
    let mut engine = evolution::EvolutionEngine::new(config.clone());
    engine.evolve(project_root)?;
    Ok(())
}
