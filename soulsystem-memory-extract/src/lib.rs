pub mod extractor;
pub mod nlp;
pub mod storage;

use anyhow::Result;
use std::path::PathBuf;
use tracing::info;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryExtractConfig {
    pub db_path: String,
    pub wiki_path: String,
    pub deduplicate: bool,
}
impl Default for MemoryExtractConfig {
    fn default() -> Self {
        Self {
            db_path: "memory.db".into(),
            wiki_path: "wiki/patterns.md".into(),
            deduplicate: true,
        }
    }
}
pub async fn run(config: MemoryExtractConfig, memory_dir: &PathBuf) -> Result<()> {
    info!("Running memory extraction from {:?}", memory_dir);
    let mut extractor = extractor::FactExtractor::new(config.clone());
    let facts = extractor.extract_all(memory_dir)?;
    storage::persist_results(&facts, &config).await?;
    Ok(())
}
