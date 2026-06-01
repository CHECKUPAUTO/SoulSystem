use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Configuration principale chargée depuis un fichier TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub evolution: EvolutionConfig,
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub evaluator: EvaluatorConfig,
    /// v4.2 — persistent storage (SQLite + WAL).
    #[serde(default)]
    pub persistence: PersistenceConfig,
    /// v4.2 — semantic embeddings client (TRIBE-compatible).
    #[serde(default)]
    pub embedding: crate::embedding::EmbeddingConfig,
    /// v4.2 — nCPU fast pre-filter.
    #[serde(default)]
    pub ncpu: crate::ncpu_filter::NcpuConfig,
    /// v4.2 — quality-diversity options (Map-Elites).
    #[serde(default)]
    pub map_elites: MapElitesConfig,
    /// v4.2 — cost guard.
    #[serde(default)]
    pub cost: CostConfig,
    /// v4.3 — LLM response cache.
    #[serde(default)]
    pub cache: crate::cache::CacheConfig,
    /// v4.3 — distributed islands.
    #[serde(default)]
    pub distributed: crate::distributed::DistributionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistenceConfig {
    /// Enable SQLite persistent storage.
    pub enabled: bool,
    /// Path to the SQLite database file.
    pub sqlite_path: String,
    /// Path to the JSONL WAL file (empty = `<output_dir>/wal.jsonl`).
    pub wal_path: String,
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sqlite_path: "openevolve_output/openevolve.db".into(),
            wal_path: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapElitesConfig {
    /// Maintain a Map-Elites archive alongside NSGA-II.
    pub enabled: bool,
    /// Probability of picking a parent from Map-Elites (vs NSGA-II / island).
    pub sample_probability: f64,
}

impl Default for MapElitesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sample_probability: 0.20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostConfig {
    /// Hard ceiling in USD; the loop aborts gracefully when exceeded.
    pub max_usd: f64,
    /// Log a warning when usage crosses this fraction of `max_usd`.
    pub warn_fraction: f64,
}

impl Default for CostConfig {
    fn default() -> Self {
        Self {
            max_usd: 0.0,
            warn_fraction: 0.8,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionConfig {
    pub max_iterations: u32,
    pub quick_iterations: u32,
    pub checkpoint_interval: u32,
    pub log_level: String,
}

impl Default for EvolutionConfig {
    fn default() -> Self {
        Self {
            max_iterations: 200,
            quick_iterations: 50,
            checkpoint_interval: 10,
            log_level: "info".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Modèle principal pour la génération de mutations (65 % du trafic)
    pub primary_model: String,
    /// Modèle secondaire pour le feedback / évaluation (35 %)
    pub secondary_model: String,
    pub primary_weight: f64,
    pub api_base: String,
    pub temperature: f64,
    pub max_tokens: u32,
    pub timeout_secs: u64,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            primary_model: "qwen2.5-coder:3b".into(),
            secondary_model: "qwen2.5-coder:3b".into(),
            primary_weight: 0.65,
            api_base: "http://localhost:11434/api".into(),
            temperature: 0.8,
            max_tokens: 4096,
            timeout_secs: 120,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub population_size: usize,
    pub num_islands: usize,
    pub migration_interval: u32,
    pub elite_fraction: f64,
    pub output_dir: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            population_size: 100,
            num_islands: 4,
            migration_interval: 20,
            elite_fraction: 0.1,
            output_dir: "openevolve_output".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluatorConfig {
    /// Binaire Rust de l'évaluateur (remplace le script Python)
    pub evaluator_bin: String,
    pub timeout_secs: u64,
    pub sandbox: bool,
}

impl Default for EvaluatorConfig {
    fn default() -> Self {
        Self {
            evaluator_bin: "./evaluator".into(),
            timeout_secs: 30,
            sandbox: true,
        }
    }
}

impl Config {
    /// Charge depuis un fichier TOML.
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Impossible de lire {:?}", path))?;
        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Erreur de parsing TOML dans {:?}", path))?;
        Ok(config)
    }

    /// Config par défaut (si aucun fichier fourni).
    pub fn default_config() -> Self {
        Self {
            evolution: EvolutionConfig::default(),
            llm: LlmConfig::default(),
            database: DatabaseConfig::default(),
            evaluator: EvaluatorConfig::default(),
            persistence: PersistenceConfig::default(),
            embedding: crate::embedding::EmbeddingConfig::default(),
            ncpu: crate::ncpu_filter::NcpuConfig::default(),
            map_elites: MapElitesConfig::default(),
            cost: CostConfig::default(),
            cache: crate::cache::CacheConfig::default(),
            distributed: crate::distributed::DistributionConfig::default(),
        }
    }

    /// Sérialise en TOML pour debug / export.
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).context("Sérialisation TOML échouée")
    }
}
