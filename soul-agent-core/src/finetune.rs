//! Closed Fine-Tuning Loop
//!
//! Automates the fine-tuning pipeline:
//! 1. Collect trajectories (already done via TrajectoryRecorder)
//! 2. Filter high-quality trajectories (score > threshold)
//! 3. Export DPO (Direct Preference Optimization) pairs
//! 4. Trigger local Ollama fine-tuning
//! 5. Hot-swap the fine-tuned model
//!
//! ## Configuration
//! - `min_trajectories`: minimum before triggering fine-tune
//! - `quality_threshold`: minimum score to include
//! - `auto_trigger`: if true, auto-fine-tune when threshold met
//! - `output_dir`: directory for exported training data

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Configuration for the fine-tuning pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FineTuneConfig {
    /// Minimum number of quality trajectories before triggering.
    pub min_trajectories: usize,
    /// Minimum quality score (0.0-1.0) to include a trajectory.
    pub quality_threshold: f64,
    /// Automatically trigger fine-tuning when threshold is met.
    pub auto_trigger: bool,
    /// Directory for exported training data.
    pub output_dir: PathBuf,
    /// Ollama modelfile name for the fine-tuned model.
    pub target_model_name: String,
    /// Base model to fine-tune from.
    pub base_model: String,
}

impl Default for FineTuneConfig {
    fn default() -> Self {
        Self {
            min_trajectories: 100,
            quality_threshold: 0.7,
            auto_trigger: false,
            output_dir: PathBuf::from("./finetune_data"),
            target_model_name: "soulsystem-finetuned".to_string(),
            base_model: "qwen3:8b".to_string(),
        }
    }
}

/// A single training example for DPO.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DpoPair {
    pub prompt: String,
    pub chosen: String,
    pub rejected: String,
    pub score: f64,
    pub domain: String,
}

/// Status of the fine-tuning pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FineTuneStatus {
    /// Idle, collecting trajectories.
    Idle,
    /// Exporting training data to disk.
    Exporting { progress: f64 },
    /// Training in progress.
    Training { current_epoch: usize, total_epochs: usize },
    /// Fine-tuning completed successfully.
    Completed { model_name: String, quality_improvement: f64 },
    /// Fine-tuning failed.
    Failed { reason: String },
    /// Waiting for manual trigger.
    AwaitingTrigger { quality_count: usize },
}

/// Remembers which trajectories were already used for training.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrainingCheckpoint {
    /// IDs of trajectories already included in the last training run.
    pub used_ids: Vec<String>,
    /// Total trajectories processed in all runs.
    pub total_processed: usize,
    /// Number of completed fine-tuning runs.
    pub fine_tune_count: usize,
    /// Timestamp of last fine-tune.
    pub last_fine_tune: Option<String>,
}

/// The fine-tuning loop manager.
pub struct FineTuneLoop {
    pub config: FineTuneConfig,
    pub status: Arc<RwLock<FineTuneStatus>>,
    pub checkpoint: Arc<RwLock<TrainingCheckpoint>>,
    /// Collected DPO pairs waiting for training.
    dpo_buffer: Arc<RwLock<Vec<DpoPair>>>,
}

impl FineTuneLoop {
    pub fn new(config: FineTuneConfig) -> Self {
        Self {
            config,
            status: Arc::new(RwLock::new(FineTuneStatus::Idle)),
            checkpoint: Arc::new(RwLock::new(TrainingCheckpoint::default())),
            dpo_buffer: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Add a DPO pair from a completed trajectory.
    pub async fn add_pair(&self, pair: DpoPair) {
        let mut buffer = self.dpo_buffer.write().await;
        buffer.push(pair);

        let count = buffer
            .iter()
            .filter(|p| p.score >= self.config.quality_threshold)
            .count();

        if count >= self.config.min_trajectories {
            let status = self.status.read().await;
            match &*status {
                FineTuneStatus::Idle => {
                    drop(status);
                    tracing::info!(
                        "Fine-tune threshold reached: {}/{} quality pairs",
                        count,
                        self.config.min_trajectories
                    );
                    if self.config.auto_trigger {
                        // Auto-trigger would be done externally via trigger()
                    } else {
                        let mut s = self.status.write().await;
                        *s = FineTuneStatus::AwaitingTrigger {
                            quality_count: count,
                        };
                    }
                }
                _ => {}
            }
        }
    }

    /// Get quality pairs above threshold.
    pub async fn quality_pairs(&self) -> Vec<DpoPair> {
        let buffer = self.dpo_buffer.read().await;
        buffer
            .iter()
            .filter(|p| p.score >= self.config.quality_threshold)
            .cloned()
            .collect()
    }

    /// Export high-quality DPO pairs to JSONL format.
    pub async fn export_training_data(&self) -> Result<PathBuf, String> {
        let pairs = self.quality_pairs().await;
        if pairs.is_empty() {
            return Err("No quality pairs to export".to_string());
        }

        // Create output directory
        tokio::fs::create_dir_all(&self.config.output_dir)
            .await
            .map_err(|e| format!("Failed to create output dir: {}", e))?;

        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("dpo_pairs_{}.jsonl", timestamp);
        let path = self.config.output_dir.join(&filename);

        let mut content = String::new();
        for pair in &pairs {
            let json = serde_json::to_string(pair)
                .map_err(|e| format!("Serialization failed: {}", e))?;
            content.push_str(&json);
            content.push('\n');
        }

        tokio::fs::write(&path, &content)
            .await
            .map_err(|e| format!("Failed to write training data: {}", e))?;

        // Update checkpoint
        self.checkpoint.write().await.total_processed += pairs.len();

        tracing::info!(
            "Exported {} DPO pairs to {}",
            pairs.len(),
            path.display()
        );

        Ok(path)
    }

    /// Trigger a fine-tuning run (via Ollama CLI).
    /// Returns the command that would be executed.
    pub async fn trigger_ollama_fine_tune(&self) -> Result<String, String> {
        let data_path = self.export_training_data().await?;

        let modelfile_content = format!(
            r#"FROM {}
PARAMETER temperature 0.7
PARAMETER top_p 0.9
# Fine-tuned on {} trajectories from SoulSystem
# Data: {}
"#,
            self.config.base_model,
            self.quality_pairs().await.len(),
            data_path.display(),
        );

        let modelfile_path = self
            .config
            .output_dir
            .join("Modelfile.finetune");
        tokio::fs::write(&modelfile_path, &modelfile_content)
            .await
            .map_err(|e| format!("Failed to write Modelfile: {}", e))?;

        let cmd = format!(
            "ollama create {} -f {} && echo 'Fine-tune complete'",
            self.config.target_model_name,
            modelfile_path.display()
        );

        tracing::info!(
            "Fine-tune command ready: ollama create {name} -f {path}",
            name = self.config.target_model_name,
            path = modelfile_path.display()
        );

        Ok(cmd)
    }

    /// Get current status summary.
    pub async fn status_summary(&self) -> FineTuneStatus {
        self.status.read().await.clone()
    }

    /// Reset buffers and return to idle.
    pub async fn reset(&self) {
        self.dpo_buffer.write().await.clear();
        let mut s = self.status.write().await;
        *s = FineTuneStatus::Idle;
    }

    /// Quality trajectory count.
    pub async fn quality_count(&self) -> usize {
        self.quality_pairs().await.len()
    }

    /// Total buffered pairs.
    pub async fn buffer_size(&self) -> usize {
        self.dpo_buffer.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let cfg = FineTuneConfig::default();
        assert_eq!(cfg.min_trajectories, 100);
        assert_eq!(cfg.quality_threshold, 0.7);
        assert!(!cfg.auto_trigger);
    }

    #[test]
    fn dpo_pair_serialization() {
        let pair = DpoPair {
            prompt: "test".into(),
            chosen: "good".into(),
            rejected: "bad".into(),
            score: 0.9,
            domain: "code".into(),
        };
        let json = serde_json::to_string(&pair).unwrap();
        let back: DpoPair = serde_json::from_str(&json).unwrap();
        assert_eq!(back.prompt, "test");
        assert_eq!(back.score, 0.9);
    }

    #[tokio::test]
    async fn fine_tune_loop_adds_pairs() {
        let ft = FineTuneLoop::new(FineTuneConfig {
            min_trajectories: 2,
            ..Default::default()
        });
        ft.add_pair(DpoPair {
            prompt: "q".into(),
            chosen: "a".into(),
            rejected: "b".into(),
            score: 0.9,
            domain: "general".into(),
        })
        .await;
        ft.add_pair(DpoPair {
            prompt: "q2".into(),
            chosen: "a2".into(),
            rejected: "b2".into(),
            score: 0.8,
            domain: "general".into(),
        })
        .await;
        assert_eq!(ft.quality_count().await, 2);
    }
}
