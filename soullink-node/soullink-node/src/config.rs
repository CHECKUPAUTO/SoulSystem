//! Configuration — TOML-backed constants for meta-cortex, reproduction,
//! and self-model hyperparameters. Loaded at startup from `soullink.toml`
//! or environment variable `SOULLINK_CONFIG`.

use serde::{Deserialize, Serialize};
use std::path::Path;

// ── Top-level config ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoulLinkConfig {
    #[serde(default)]
    pub meta: MetaConfig,
    #[serde(default)]
    pub reproduction: ReproductionConfig,
}

impl Default for SoulLinkConfig {
    fn default() -> Self {
        Self {
            meta: MetaConfig::default(),
            reproduction: ReproductionConfig::default(),
        }
    }
}

impl SoulLinkConfig {
    /// Load from `soullink.toml` (or `SOULLINK_CONFIG` env var path).
    /// Falls back to defaults if file is missing or invalid.
    pub fn load() -> Self {
        let path = std::env::var("SOULLINK_CONFIG")
            .unwrap_or_else(|_| "soullink.toml".to_string());

        if !Path::new(&path).exists() {
            tracing::info!("📄 Aucun fichier de config trouvé à {path}, utilisation des valeurs par défaut");
            return Self::default();
        }

        match std::fs::read_to_string(&path) {
            Ok(toml_str) => match toml::from_str::<Self>(&toml_str) {
                Ok(cfg) => {
                    tracing::info!("📄 Configuration chargée depuis {path}");
                    cfg
                }
                Err(e) => {
                    tracing::warn!("⚠️  Config TOML invalide ({e}), fallback aux valeurs par défaut");
                    Self::default()
                }
            },
            Err(e) => {
                tracing::warn!("⚠️  Impossible de lire {path} ({e}), fallback aux valeurs par défaut");
                Self::default()
            }
        }
    }

    /// Write a default `soullink.toml` template to disk.
    pub fn write_template(path: &str) -> Result<(), String> {
        let cfg = Self::default();
        let toml_str = toml::to_string_pretty(&cfg)
            .map_err(|e| format!("TOML serialization failed: {e}"))?;
        std::fs::write(path, toml_str)
            .map_err(|e| format!("Failed to write template: {e}"))?;
        tracing::info!("📄 Template de config écrit à {path}");
        Ok(())
    }
}

// ── Meta-cortex config ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaConfig {
    /// Maximum observations retained for loop detection.
    #[serde(default = "default_max_observations")]
    pub max_observations: usize,
    /// Consecutive pattern repeats before flagging a loop.
    #[serde(default = "default_loop_detection_threshold")]
    pub loop_detection_threshold: usize,
    /// Cooldown ticks between meta-actions.
    #[serde(default = "default_cooldown")]
    pub default_cooldown: u64,
    /// Max meta-actions per observation window.
    #[serde(default = "default_max_actions_per_window")]
    pub max_actions_per_window: usize,
    /// Window size for computing rolling success rate.
    #[serde(default = "default_success_rate_window")]
    pub success_rate_window: usize,
    /// Maximum recursive depth (prevents infinite meta-cascades).
    #[serde(default = "default_max_recursive_depth")]
    pub max_recursive_depth: u32,
    /// Ticks to wait before measuring short-term outcome.
    #[serde(default = "default_short_term_delay")]
    pub short_term_delay: u64,
    /// Ticks to wait before measuring long-term outcome.
    #[serde(default = "default_long_term_delay")]
    pub long_term_delay: u64,
    /// Minimum improvement in fitness to count intervention as success.
    #[serde(default = "default_min_fitness_improvement")]
    pub min_fitness_improvement: f64,
    /// Maximum pending interventions tracked simultaneously.
    #[serde(default = "default_max_pending_interventions")]
    pub max_pending_interventions: usize,
    /// Dimension of the brain state summary used by the self-model.
    #[serde(default = "default_self_model_input_dim")]
    pub self_model_input_dim: usize,
    /// Hidden layer size for the self-model MLP.
    #[serde(default = "default_self_model_hidden_dim")]
    pub self_model_hidden_dim: usize,
    /// Default learning rate for online SGD.
    #[serde(default = "default_self_model_lr")]
    pub self_model_default_lr: f64,
    /// How often (in ticks) the self-model runs a learning update.
    #[serde(default = "default_self_model_learn_interval")]
    pub self_model_learn_interval: u64,
    /// Maximum prediction horizon for counterfactuals.
    #[serde(default = "default_max_prediction_horizon")]
    pub max_prediction_horizon: usize,
}

impl Default for MetaConfig {
    fn default() -> Self {
        Self {
            max_observations: default_max_observations(),
            loop_detection_threshold: default_loop_detection_threshold(),
            default_cooldown: default_cooldown(),
            max_actions_per_window: default_max_actions_per_window(),
            success_rate_window: default_success_rate_window(),
            max_recursive_depth: default_max_recursive_depth(),
            short_term_delay: default_short_term_delay(),
            long_term_delay: default_long_term_delay(),
            min_fitness_improvement: default_min_fitness_improvement(),
            max_pending_interventions: default_max_pending_interventions(),
            self_model_input_dim: default_self_model_input_dim(),
            self_model_hidden_dim: default_self_model_hidden_dim(),
            self_model_default_lr: default_self_model_lr(),
            self_model_learn_interval: default_self_model_learn_interval(),
            max_prediction_horizon: default_max_prediction_horizon(),
        }
    }
}

fn default_max_observations() -> usize { 256 }
fn default_loop_detection_threshold() -> usize { 5 }
fn default_cooldown() -> u64 { 50 }
fn default_max_actions_per_window() -> usize { 5 }
fn default_success_rate_window() -> usize { 20 }
fn default_max_recursive_depth() -> u32 { 3 }
fn default_short_term_delay() -> u64 { 10 }
fn default_long_term_delay() -> u64 { 50 }
fn default_min_fitness_improvement() -> f64 { 0.01 }
fn default_max_pending_interventions() -> usize { 8 }
fn default_self_model_input_dim() -> usize { 16 }
fn default_self_model_hidden_dim() -> usize { 32 }
fn default_self_model_lr() -> f64 { 0.01 }
fn default_self_model_learn_interval() -> u64 { 5 }
fn default_max_prediction_horizon() -> usize { 50 }

// ── Reproduction config ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReproductionConfig {
    /// Enable automatic reproduction scheduling.
    #[serde(default = "default_true")]
    pub auto_scheduler_enabled: bool,
    /// Fitness must be above this threshold to trigger reproduction.
    #[serde(default = "default_repro_fitness_threshold")]
    pub fitness_threshold: f64,
    /// Number of consecutive ticks above threshold required.
    #[serde(default = "default_repro_consecutive_ticks")]
    pub consecutive_ticks_required: u64,
    /// Trigger emergency reproduction on Panic+SOS as backup.
    #[serde(default = "default_true")]
    pub panic_sos_backup: bool,
    /// Default alpha for sexual crossover (0.0–1.0).
    #[serde(default = "default_crossover_alpha")]
    pub crossover_alpha: f32,
    /// Cooldown between auto-reproductions (ticks).
    #[serde(default = "default_repro_cooldown")]
    pub cooldown_ticks: u64,
}

impl Default for ReproductionConfig {
    fn default() -> Self {
        Self {
            auto_scheduler_enabled: true,
            fitness_threshold: default_repro_fitness_threshold(),
            consecutive_ticks_required: default_repro_consecutive_ticks(),
            panic_sos_backup: true,
            crossover_alpha: 0.5,
            cooldown_ticks: default_repro_cooldown(),
        }
    }
}

fn default_true() -> bool { true }
fn default_repro_fitness_threshold() -> f64 { 0.7 }
fn default_repro_consecutive_ticks() -> u64 { 10 }
fn default_crossover_alpha() -> f32 { 0.5 }
fn default_repro_cooldown() -> u64 { 1000 }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_roundtrip() {
        let cfg = SoulLinkConfig::default();
        let toml_str = toml::to_string_pretty(&cfg).expect("serialize");
        let decoded: SoulLinkConfig = toml::from_str(&toml_str).expect("deserialize");
        assert_eq!(decoded.meta.max_observations, cfg.meta.max_observations);
        assert_eq!(decoded.meta.self_model_input_dim, cfg.meta.self_model_input_dim);
        assert_eq!(decoded.reproduction.fitness_threshold, cfg.reproduction.fitness_threshold);
    }

    #[test]
    fn test_partial_config() {
        let partial = r#"
[meta]
max_observations = 128
loop_detection_threshold = 3

[reproduction]
fitness_threshold = 0.9
"#;
        let cfg: SoulLinkConfig = toml::from_str(partial).expect("parse");
        assert_eq!(cfg.meta.max_observations, 128);
        assert_eq!(cfg.meta.loop_detection_threshold, 3);
        // Unspecified fields should use defaults
        assert_eq!(cfg.meta.default_cooldown, 50);
        assert_eq!(cfg.reproduction.fitness_threshold, 0.9);
        assert_eq!(cfg.reproduction.consecutive_ticks_required, 10);
    }
}
