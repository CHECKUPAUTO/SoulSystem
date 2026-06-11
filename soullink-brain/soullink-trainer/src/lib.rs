//! soullink-trainer — Fine-tuning pipeline for SoulSystem.
//!
//! Harvests successful metacognition trajectories, filters by quality,
//! and exports them as JSONL training data for Ollama fine-tuning.
//!
//! # Pipeline
//!
//! ```text
//!   Orchestrator
//!       │
//!       ▼
//!   TrajectoryRecorder  ──(JSONL)──►  trajectory.jsonl
//!       │
//!       ▼
//!   TrajectoryFilter    ──(quality >= threshold)──► filtered.jsonl
//!       │
//!       ▼
//!   DataExporter        ──(train/val split)──► train.jsonl + val.jsonl
//!       │
//!       ▼
//!   OllamaFineTuner     ──(POST /api/create)──► fine-tuned model
//! ```

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

// ── Trajectory Types ────────────────────────────────────────────────────

/// A single inference trajectory — the full context of one LLM interaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trajectory {
    /// Unique trajectory ID.
    pub id: String,
    /// Timestamp (RFC 3339).
    pub timestamp: String,
    /// The model used (e.g., "qwen2.5-coder:3b").
    pub model: String,
    /// Quantization level (e.g., "Q4_K_M").
    pub quantization: String,
    /// System prompt sent to the model.
    pub system_prompt: String,
    /// User query / prompt.
    pub prompt: String,
    /// Model response.
    pub response: String,
    /// Action taken by the orchestrator based on this response.
    pub action: String,
    /// Model's confidence in its response [0.0, 1.0].
    pub confidence: f32,
    /// Quality score from metacognition [0.0, 1.0].
    pub quality_score: f32,
    /// Quality category: Excellent, Good, Neutral, Poor, Catastrophic.
    pub quality_category: String,
    /// Semantic loss (1 - cosine_similarity).
    pub loss: f32,
    /// Cosine similarity between query and response embeddings.
    pub cosine_similarity: f32,
    /// Number of concepts updated in memory graph.
    pub concepts_updated: usize,
    /// Whether the action succeeded.
    pub success: bool,
    /// Metacognition cycle number.
    pub cycle: u64,
    /// System tick number.
    pub tick: u64,
    /// Energy delta after action.
    pub energy_delta: f64,
    /// CPU delta (%) after action.
    pub cpu_delta: f32,
    /// Alerts delta after action.
    pub alerts_delta: i32,
    /// Additional metadata.
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Trajectory {
    /// Create a new trajectory with a generated ID.
    pub fn new(model: &str, quantization: &str, prompt: &str, response: &str) -> Self {
        Self {
            id: uuid_simple(),
            timestamp: Utc::now().to_rfc3339(),
            model: model.to_string(),
            quantization: quantization.to_string(),
            system_prompt: String::new(),
            prompt: prompt.to_string(),
            response: response.to_string(),
            action: String::new(),
            confidence: 0.0,
            quality_score: 0.0,
            quality_category: "Unknown".into(),
            loss: 0.0,
            cosine_similarity: 1.0,
            concepts_updated: 0,
            success: false,
            cycle: 0,
            tick: 0,
            energy_delta: 0.0,
            cpu_delta: 0.0,
            alerts_delta: 0,
            metadata: HashMap::new(),
        }
    }

    /// Whether this trajectory is suitable for training.
    pub fn is_usable(&self) -> bool {
        !self.prompt.is_empty() && !self.response.is_empty() && self.quality_score > 0.0
    }
}

/// A DPO (Direct Preference Optimization) training pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DpoPair {
    /// The prompt.
    pub prompt: String,
    /// The chosen (high-quality) response.
    pub chosen: String,
    /// The rejected (low-quality) response.
    pub rejected: String,
    /// Quality scores for reference.
    pub chosen_score: f32,
    pub rejected_score: f32,
}

// ── TrajectoryRecorder ──────────────────────────────────────────────────

/// Records inference trajectories to a JSONL file.
pub struct TrajectoryRecorder {
    path: PathBuf,
    file: Option<File>,
    count: u64,
}

impl TrajectoryRecorder {
    /// Create a new recorder writing to the given path.
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .context("failed to open trajectory file")?;

        Ok(Self {
            path,
            file: Some(file),
            count: 0,
        })
    }

    /// Record a trajectory.
    pub fn record(&mut self, traj: &Trajectory) -> Result<()> {
        if let Some(ref mut file) = self.file {
            let mut line = serde_json::to_string(traj).context("failed to serialize trajectory")?;
            line.push('\n');
            file.write_all(line.as_bytes())
                .context("failed to write trajectory")?;
            file.flush()?;
            self.count += 1;
        }
        Ok(())
    }

    /// Number of trajectories recorded in this session.
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Path to the trajectory file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

// ── TrajectoryFilter ────────────────────────────────────────────────────

/// Configuration for filtering trajectories.
#[derive(Debug, Clone)]
pub struct FilterConfig {
    /// Minimum quality score to include (0.0 - 1.0).
    pub min_quality: f32,
    /// Minimum confidence to include.
    pub min_confidence: f32,
    /// Maximum semantic loss to include.
    pub max_loss: f32,
    /// Only include successful actions.
    pub require_success: bool,
    /// Maximum age in days (0 = no limit).
    pub max_age_days: u32,
    /// Model to filter by (empty = any).
    pub model_filter: String,
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            min_quality: 0.6,
            min_confidence: 0.5,
            max_loss: 0.5,
            require_success: true,
            max_age_days: 30,
            model_filter: String::new(),
        }
    }
}

/// Filters trajectories by quality criteria.
pub struct TrajectoryFilter {
    config: FilterConfig,
}

impl TrajectoryFilter {
    pub fn new(config: FilterConfig) -> Self {
        Self { config }
    }

    /// Check if a trajectory passes the filter.
    pub fn passes(&self, traj: &Trajectory) -> bool {
        if traj.quality_score < self.config.min_quality {
            return false;
        }
        if traj.confidence < self.config.min_confidence {
            return false;
        }
        if traj.loss > self.config.max_loss {
            return false;
        }
        if self.config.require_success && !traj.success {
            return false;
        }
        if !self.config.model_filter.is_empty() && traj.model != self.config.model_filter {
            return false;
        }
        // Age check
        if self.config.max_age_days > 0 {
            if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&traj.timestamp) {
                let age = Utc::now() - ts.with_timezone(&Utc);
                if age.num_days() > self.config.max_age_days as i64 {
                    return false;
                }
            }
        }
        true
    }

    /// Filter a list of trajectories.
    pub fn filter(&self, trajectories: &[Trajectory]) -> Vec<Trajectory> {
        trajectories
            .iter()
            .filter(|t| self.passes(t))
            .cloned()
            .collect()
    }

    /// Filter a JSONL file and write results to another file.
    pub fn filter_file(&self, input: &Path, output: &Path) -> Result<FilterStats> {
        let file = File::open(input).context("failed to open input file")?;
        let reader = BufReader::new(file);
        let mut writer = File::create(output).context("failed to create output file")?;

        let mut stats = FilterStats::default();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            stats.total += 1;

            match serde_json::from_str::<Trajectory>(&line) {
                Ok(traj) => {
                    if self.passes(&traj) {
                        stats.passed += 1;
                        let mut out_line = serde_json::to_string(&traj)?;
                        out_line.push('\n');
                        writer.write_all(out_line.as_bytes())?;
                    } else {
                        stats.filtered += 1;
                    }
                }
                Err(e) => {
                    stats.parse_errors += 1;
                    warn!("failed to parse trajectory: {}", e);
                }
            }
        }

        writer.flush()?;
        info!(
            "Filter: {}/{} trajectories passed ({} filtered, {} errors)",
            stats.passed, stats.total, stats.filtered, stats.parse_errors
        );

        Ok(stats)
    }
}

/// Statistics from filtering.
#[derive(Debug, Default, Clone)]
pub struct FilterStats {
    pub total: u64,
    pub passed: u64,
    pub filtered: u64,
    pub parse_errors: u64,
}

// ── DataExporter ────────────────────────────────────────────────────────

/// Exports filtered trajectories to training format.
pub struct DataExporter {
    /// Train/val split ratio (default: 0.9 = 90% train, 10% val).
    pub split_ratio: f32,
    /// System prompt template for training.
    pub system_prompt: String,
}

impl DataExporter {
    pub fn new() -> Self {
        Self {
            split_ratio: 0.9,
            system_prompt:
                "You are SoulLink, an AI interface to a mesh of specialized neural modules.".into(),
        }
    }

    /// Export filtered trajectories to train/val JSONL files.
    ///
    /// Each line is a JSON object with `prompt` and `response` fields,
    /// suitable for Ollama fine-tuning format.
    pub fn export(
        &self,
        trajectories: &[Trajectory],
        train_path: &Path,
        val_path: &Path,
    ) -> Result<ExportStats> {
        if trajectories.is_empty() {
            warn!("No trajectories to export");
            return Ok(ExportStats::default());
        }

        // Sort by quality score descending (best first)
        let mut sorted = trajectories.to_vec();
        sorted.sort_by(|a, b| b.quality_score.partial_cmp(&a.quality_score).unwrap());

        let split_idx = (sorted.len() as f32 * self.split_ratio) as usize;
        let (train_data, val_data) = sorted.split_at(split_idx.max(1));

        // Write training data
        let mut train_writer = File::create(train_path).context("failed to create train file")?;
        for traj in train_data {
            let entry = self.trajectory_to_training_entry(traj);
            let mut line = serde_json::to_string(&entry)?;
            line.push('\n');
            train_writer.write_all(line.as_bytes())?;
        }
        train_writer.flush()?;

        // Write validation data
        let mut val_writer = File::create(val_path).context("failed to create val file")?;
        for traj in val_data {
            let entry = self.trajectory_to_training_entry(traj);
            let mut line = serde_json::to_string(&entry)?;
            line.push('\n');
            val_writer.write_all(line.as_bytes())?;
        }
        val_writer.flush()?;

        let stats = ExportStats {
            total: trajectories.len() as u64,
            train: train_data.len() as u64,
            val: val_data.len() as u64,
            avg_quality: sorted.iter().map(|t| t.quality_score).sum::<f32>() / sorted.len() as f32,
        };

        info!(
            "Export: {} train, {} val (avg quality: {:.2})",
            stats.train, stats.val, stats.avg_quality
        );

        Ok(stats)
    }

    /// Export DPO preference pairs from trajectories.
    ///
    /// Pairs high-quality and low-quality responses for the same prompt.
    pub fn export_dpo_pairs(&self, trajectories: &[Trajectory], output: &Path) -> Result<usize> {
        // Group trajectories by prompt
        let mut by_prompt: HashMap<String, Vec<&Trajectory>> = HashMap::new();
        for traj in trajectories {
            by_prompt.entry(traj.prompt.clone()).or_default().push(traj);
        }

        let mut writer = File::create(output).context("failed to create DPO file")?;
        let mut pair_count = 0;

        for (prompt, trajs) in &by_prompt {
            if trajs.len() < 2 {
                continue; // Need at least 2 responses for a pair
            }

            // Sort by quality
            let mut sorted = trajs.clone();
            sorted.sort_by(|a, b| b.quality_score.partial_cmp(&a.quality_score).unwrap());

            // Pair best with worst
            let chosen = sorted[0];
            let rejected = sorted[sorted.len() - 1];

            if chosen.quality_score <= rejected.quality_score {
                continue; // No meaningful difference
            }

            let pair = DpoPair {
                prompt: format!(
                    "<system>{}</system>\n<user>{}</user>",
                    chosen.system_prompt, prompt
                ),
                chosen: chosen.response.clone(),
                rejected: rejected.response.clone(),
                chosen_score: chosen.quality_score,
                rejected_score: rejected.quality_score,
            };

            let mut line = serde_json::to_string(&pair)?;
            line.push('\n');
            writer.write_all(line.as_bytes())?;
            pair_count += 1;
        }

        writer.flush()?;
        info!("Exported {} DPO pairs", pair_count);
        Ok(pair_count)
    }

    fn trajectory_to_training_entry(&self, traj: &Trajectory) -> serde_json::Value {
        serde_json::json!({
            "prompt": format!(
                "<system>{}</system>\n<user>{}</user>",
                if traj.system_prompt.is_empty() { &self.system_prompt } else { &traj.system_prompt },
                traj.prompt
            ),
            "response": traj.response,
            "quality_score": traj.quality_score,
            "action": traj.action,
            "model": traj.model,
        })
    }
}

impl Default for DataExporter {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics from export.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ExportStats {
    pub total: u64,
    pub train: u64,
    pub val: u64,
    pub avg_quality: f32,
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:x}", t)
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_trajectory(quality: f32, success: bool, model: &str) -> Trajectory {
        let mut t = Trajectory::new(model, "Q4_K_M", "test prompt", "test response");
        t.quality_score = quality;
        t.success = success;
        t.confidence = 0.8;
        t.loss = 0.2;
        t.quality_category = if quality >= 0.8 {
            "Excellent"
        } else if quality >= 0.6 {
            "Good"
        } else {
            "Poor"
        }
        .into();
        t.action = "optimize_system".into();
        t
    }

    #[test]
    fn test_trajectory_recorder() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("traj.jsonl");

        let mut recorder = TrajectoryRecorder::new(&path).unwrap();
        let traj = make_trajectory(0.8, true, "qwen2.5-coder:3b");
        recorder.record(&traj).unwrap();
        recorder.record(&traj).unwrap();

        assert_eq!(recorder.count(), 2);

        // Verify file content
        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_trajectory_filter() {
        let filter = TrajectoryFilter::new(FilterConfig {
            min_quality: 0.6,
            min_confidence: 0.5,
            max_loss: 0.5,
            require_success: true,
            max_age_days: 0, // No age limit
            model_filter: "qwen2.5-coder:3b".into(),
        });

        let good = make_trajectory(0.8, true, "qwen2.5-coder:3b");
        let poor = make_trajectory(0.3, true, "qwen2.5-coder:3b");
        let failed = make_trajectory(0.8, false, "qwen2.5-coder:3b");
        let wrong_model = make_trajectory(0.8, true, "gemma4:31b");

        assert!(filter.passes(&good));
        assert!(!filter.passes(&poor));
        assert!(!filter.passes(&failed));
        assert!(!filter.passes(&wrong_model));
    }

    #[test]
    fn test_data_exporter() {
        let dir = tempfile::TempDir::new().unwrap();
        let train_path = dir.path().join("train.jsonl");
        let val_path = dir.path().join("val.jsonl");

        let trajs: Vec<Trajectory> = (0..20)
            .map(|i| make_trajectory(0.5 + (i as f32 * 0.025), true, "qwen2.5-coder:3b"))
            .collect();

        let exporter = DataExporter::new();
        let stats = exporter.export(&trajs, &train_path, &val_path).unwrap();

        assert_eq!(stats.total, 20);
        assert!(stats.train > 0);
        assert!(stats.val > 0);
        assert_eq!(stats.train + stats.val, 20);
    }

    #[test]
    fn test_dpo_export() {
        let dir = tempfile::TempDir::new().unwrap();
        let dpo_path = dir.path().join("dpo.jsonl");

        let mut trajs = Vec::new();
        // Two responses to the same prompt, different quality
        let mut t1 = make_trajectory(0.9, true, "qwen2.5-coder:3b");
        t1.prompt = "same prompt".into();
        trajs.push(t1);

        let mut t2 = make_trajectory(0.3, true, "qwen2.5-coder:3b");
        t2.prompt = "same prompt".into();
        trajs.push(t2);

        // Different prompt (won't pair)
        let mut t3 = make_trajectory(0.8, true, "qwen2.5-coder:3b");
        t3.prompt = "other prompt".into();
        trajs.push(t3);

        let exporter = DataExporter::new();
        let pairs = exporter.export_dpo_pairs(&trajs, &dpo_path).unwrap();

        assert_eq!(pairs, 1); // Only one pair (same prompt)

        let content = std::fs::read_to_string(&dpo_path).unwrap();
        let pair: DpoPair = serde_json::from_str(content.trim()).unwrap();
        assert!(pair.chosen_score > pair.rejected_score);
    }

    #[test]
    fn test_filter_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let input = dir.path().join("all.jsonl");
        let output = dir.path().join("filtered.jsonl");

        // Write test data
        let mut file = File::create(&input).unwrap();
        for q in [0.9, 0.3, 0.8, 0.1, 0.7] {
            let t = make_trajectory(q, true, "qwen2.5-coder:3b");
            let mut line = serde_json::to_string(&t).unwrap();
            line.push('\n');
            file.write_all(line.as_bytes()).unwrap();
        }
        drop(file);

        let filter = TrajectoryFilter::new(FilterConfig {
            min_quality: 0.6,
            ..Default::default()
        });

        let stats = filter.filter_file(&input, &output).unwrap();
        assert_eq!(stats.total, 5);
        assert_eq!(stats.passed, 3); // 0.9, 0.8, 0.7
        assert_eq!(stats.filtered, 2); // 0.3, 0.1
    }
}
