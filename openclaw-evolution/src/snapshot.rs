use crate::agent::Population;
use crate::concept::ConceptEngine;
use crate::memory::MemoryEntry;
use crate::meta_language::MetaLanguage;
use crate::ollama::InferenceMode;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub cycle: u64,
    pub population: Population,
    pub memory_data: Vec<(String, MemoryEntry)>,
    pub il_version: u64,
    pub meta_language: MetaLanguage,
    pub concept_engine: ConceptEngine,
    pub inference_mode: InferenceMode,
    pub metrics: SystemMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub avg_fitness: f32,
    pub best_fitness: f32,
    pub population_size: usize,
    pub agents_compiling: usize,
    pub agents_running: usize,
    pub memory_entries: usize,
    pub concepts_count: usize,
    pub generation: u64,
}

pub struct SnapshotManager {
    snapshot_dir: PathBuf,
    max_snapshots: usize,
}

impl SnapshotManager {
    pub fn new(snapshot_dir: PathBuf, max_snapshots: usize) -> Result<Self> {
        std::fs::create_dir_all(&snapshot_dir)?;
        Ok(Self {
            snapshot_dir,
            max_snapshots,
        })
    }

    pub fn save(&self, snapshot: &Snapshot) -> Result<PathBuf> {
        let filename = format!(
            "snapshot_cycle_{:06}_{}.json",
            snapshot.cycle,
            snapshot.timestamp.format("%Y%m%d_%H%M%S")
        );
        let path = self.snapshot_dir.join(&filename);
        let json = serde_json::to_string_pretty(snapshot)?;
        std::fs::write(&path, json)?;

        info!(
            "Snapshot: cycle={}, fitness={:.3}, compile={}/{}, fichier={}",
            snapshot.cycle,
            snapshot.metrics.avg_fitness,
            snapshot.metrics.agents_compiling,
            snapshot.metrics.population_size,
            filename
        );

        self.cleanup()?;
        Ok(path)
    }

    pub fn load_latest(&self) -> Result<Option<Snapshot>> {
        let mut snapshots = self.list_snapshots()?;
        if snapshots.is_empty() {
            return Ok(None);
        }
        snapshots.sort();
        let latest = snapshots.last().unwrap();
        let path = self.snapshot_dir.join(latest);
        info!("Chargement snapshot: {}", latest);
        let json = std::fs::read_to_string(&path)?;
        let snapshot: Snapshot = serde_json::from_str(&json)?;
        Ok(Some(snapshot))
    }

    pub fn load_by_cycle(&self, cycle: u64) -> Result<Option<Snapshot>> {
        let prefix = format!("snapshot_cycle_{:06}", cycle);
        for name in self.list_snapshots()? {
            if name.starts_with(&prefix) {
                let path = self.snapshot_dir.join(&name);
                let json = std::fs::read_to_string(&path)?;
                return Ok(Some(serde_json::from_str(&json)?));
            }
        }
        Ok(None)
    }

    fn list_snapshots(&self) -> Result<Vec<String>> {
        let mut names = Vec::new();
        if self.snapshot_dir.exists() {
            for entry in std::fs::read_dir(&self.snapshot_dir)? {
                let name = entry?.file_name().to_string_lossy().to_string();
                if name.starts_with("snapshot_") && name.ends_with(".json") {
                    names.push(name);
                }
            }
        }
        Ok(names)
    }

    fn cleanup(&self) -> Result<()> {
        let mut snapshots = self.list_snapshots()?;
        if snapshots.len() <= self.max_snapshots {
            return Ok(());
        }
        snapshots.sort();
        let to_remove = snapshots.len() - self.max_snapshots;
        for name in snapshots.into_iter().take(to_remove) {
            std::fs::remove_file(self.snapshot_dir.join(&name))?;
        }
        Ok(())
    }

    pub fn create_snapshot(
        cycle: u64,
        population: &Population,
        memory_data: Vec<(String, MemoryEntry)>,
        il_version: u64,
        meta_language: &MetaLanguage,
        concept_engine: &ConceptEngine,
        inference_mode: InferenceMode,
    ) -> Snapshot {
        let agents_compiling = population
            .agents
            .iter()
            .filter(|a| a.metrics.compiles)
            .count();
        let agents_running = population
            .agents
            .iter()
            .filter(|a| a.metrics.runs_ok)
            .count();

        Snapshot {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now(),
            cycle,
            population: population.clone(),
            memory_data,
            il_version,
            meta_language: meta_language.clone(),
            concept_engine: concept_engine.clone(),
            inference_mode,
            metrics: SystemMetrics {
                avg_fitness: population.average_fitness(),
                best_fitness: population.best_agent().map(|a| a.fitness).unwrap_or(0.0),
                population_size: population.agents.len(),
                agents_compiling,
                agents_running,
                memory_entries: 0, // sera mis à jour
                concepts_count: concept_engine.concepts.len(),
                generation: population.generation,
            },
        }
    }
}
