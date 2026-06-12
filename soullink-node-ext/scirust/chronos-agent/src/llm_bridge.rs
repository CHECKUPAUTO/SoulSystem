// ==========================================================================
// llm_bridge.rs — Pont d'Observation entre ChronosAgent et les LLM réels
//
// Architecture en 2 phases (Gemini's suggestion, executed by SoulLink):
//   Phase 1 — Observation  : LLM en lecture seule, on observe ses latents
//   Phase 2 — Supervision   : LLM connecté au pipeline complet (RegretOptimizer)
//
// Ce fichier implémente Phase 1 : un wrapper candle-transformers qui :
//   - Charge un modèle local (GGUF / safetensors)
//   - Extrait les représentations latentes (last hidden state)
//   - Les expose au SemanticStore pour analyse de clustering
//   - NE modifie PAS les poids du LLM (mode read-only garanti)
// ==========================================================================

use candle_core::{Device, Result, Tensor};

// --------------------------------------------------------------------------
// LLMObserver — Phase 1 : Read-only LLM wrapper
// --------------------------------------------------------------------------

/// Configuration for loading a local LLM in observation mode.
pub struct LLMConfig {
    /// Path to the model file (GGUF or safetensors).
    pub model_path: String,
    /// Tokenizer path (tokenizer.json or equivalent).
    pub tokenizer_path: String,
    /// Embedding dimension of the model.
    pub d_model: usize,
    /// Vocabulary size.
    pub vocab_size: usize,
    /// Number of transformer layers.
    pub num_layers: usize,
    /// Number of attention heads.
    pub n_heads: usize,
    /// Head dimension.
    pub d_head: usize,
    /// Device to run on (Cpu or Cuda).
    pub device: Device,
    /// Maximum sequence length.
    pub max_seq_len: usize,
}

impl Default for LLMConfig {
    fn default() -> Self {
        Self {
            model_path: String::new(),
            tokenizer_path: String::new(),
            d_model: 2048,       // Llama-3 1B-ish default
            vocab_size: 32000,
            num_layers: 16,
            n_heads: 16,
            d_head: 128,
            device: Device::Cpu,
            max_seq_len: 2048,
        }
    }
}

/// Wrapper around a loaded LLM in observation-only mode.
///
/// # Safety guarantees
/// - No gradient computation
/// - No weight modification
/// - All forward passes are detached
pub struct LLMObserver {
    config: LLMConfig,
    /// Cached last hidden state from the most recent forward pass.
    /// Shape: (d_model,) — pooled representation of the final token.
    last_latent: Option<Vec<f64>>,
    /// Running buffer of observed latents for cluster analysis.
    latent_history: Vec<Vec<f64>>,
    /// Maximum number of latent vectors to store in history.
    history_capacity: usize,
}

impl LLMObserver {
    /// Create a new LLMObserver with the given configuration.
    /// Does NOT load the model — call `load()` separately.
    pub fn new(config: LLMConfig) -> Self {
        let history_capacity = 1024; // enough for 1024 observations before clustering
        Self {
            config,
            last_latent: None,
            latent_history: Vec::with_capacity(history_capacity),
            history_capacity,
        }
    }

    /// Load the model file into memory.
    /// In Phase 1, this is a no-op that validates the config exists.
    /// In production, this would call candle-transformers' loader.
    pub fn load(&mut self) -> std::result::Result<(), String> {
        if self.config.model_path.is_empty() {
            return Err("LLMConfig.model_path is empty — nothing to load".to_string());
        }
        if !std::path::Path::new(&self.config.model_path).exists() {
            return Err(format!("Model file not found: {}", self.config.model_path));
        }
        // TODO: wire candle-transformers ModelLoader here
        // let model = candle_transformers::models::llama::Llama::load(...)?;
        log_once("Phase 1 ready: model file validated (loading deferred)");
        Ok(())
    }

    /// Forward pass through the LLM in observation mode.
    ///
    /// Returns the last hidden state as a Vec<f64> (detached, read-only).
    /// In Phase 1, this is a mock that generates realistic embeddings.
    /// In production, this calls the actual model.
    pub fn observe(&mut self, input_text: &str) -> Result<Vec<f64>> {
        // Phase 1: mock observation — simulate a realistic latent vector
        // The real model would produce this from `input_text`.
        let latent = self.mock_forward(input_text)?;

        // Store in history
        self.latent_history.push(latent.clone());
        if self.latent_history.len() > self.history_capacity {
            self.latent_history.remove(0);
        }

        self.last_latent = Some(latent.clone());
        Ok(latent)
    }

    /// Get the most recent latent observation.
    pub fn last_latent(&self) -> Option<&[f64]> {
        self.last_latent.as_deref()
    }

    /// Get the full latent history (for clustering analysis).
    pub fn latent_history(&self) -> &[Vec<f64>] {
        &self.latent_history
    }

    /// Clear the latent history buffer.
    pub fn clear_history(&mut self) {
        self.latent_history.clear();
        self.last_latent = None;
    }

    /// Access the model configuration.
    pub fn config(&self) -> &LLMConfig {
        &self.config
    }

    // ----------------------------------------------------------------------
    // Mock forward — simulates LLM embedding for Phase 1 testing
    // ----------------------------------------------------------------------

    fn mock_forward(&self, input_text: &str) -> Result<Vec<f64>> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let d = self.config.d_model;

        // Deterministic embedding from text hash (so same text → same latent)
        let mut hasher = DefaultHasher::new();
        input_text.hash(&mut hasher);
        let seed = hasher.finish() as f64;

        // Generate d_model-dimensional vector with structure:
        // - First 64 dimensions encode content (text-derived)
        // - Next dimensions add noise (simulating attention variance)
        // - Last dimension encodes coherence score
        let mut latent = Vec::with_capacity(d);
        let mut rng_state = seed;

        for i in 0..d {
            rng_state = rng_state * 6364136223846793005f64 + 1442695040888963407f64;
            let val = if i < 64 {
                // Content dimensions: structured around text length and seed
                let base = (input_text.len() as f64 * 0.01).sin() * 0.5 + 0.5;
                let dim_mod = (i as f64 * seed * 0.1).sin().abs();
                base * dim_mod
            } else if i == d - 1 {
                // Coherence score: 0.0-1.0 based on text structure
                (input_text.len() as f64 % 50.0 / 50.0).clamp(0.1, 0.95)
            } else {
                // Noise dimensions with structure
                (seed * (i as f64).cos()).sin() * 0.3
            };
            latent.push(val);
        }

        Ok(latent)
    }
}

// --------------------------------------------------------------------------
// Cluster analyzer — validates SemanticStore readiness
// --------------------------------------------------------------------------

/// Results of a clustering analysis on observed latents.
#[derive(Debug)]
pub struct ClusterReport {
    /// Number of unique clusters detected.
    pub cluster_count: usize,
    /// Intra-cluster coherence (mean cosine similarity within clusters).
    pub intra_coherence: f64,
    /// Inter-cluster separation (mean cosine dissimilarity between centroids).
    pub inter_separation: f64,
    /// Ratio: how well-separated the clusters are (higher = better).
    pub separation_ratio: f64,
    /// Number of observations analyzed.
    pub total_observations: usize,
}

/// Analyze observed latents for cluster coherence.
/// This validates whether the SemanticStore can meaningfully index
/// the representations produced by the LLM.
pub fn analyze_clusters(observer: &LLMObserver) -> ClusterReport {
    let history = observer.latent_history();
    let n = history.len();

    if n < 2 {
        return ClusterReport {
            cluster_count: 0,
            intra_coherence: 0.0,
            inter_separation: 0.0,
            separation_ratio: 0.0,
            total_observations: n,
        };
    }

    // Simple clustering: group by similarity threshold
    // (A proper implementation would use k-means or HDBSCAN)
    use std::collections::HashMap;

    let mut centroids: HashMap<usize, (Vec<f64>, usize)> = HashMap::new();
    let threshold = 0.65;

    for vec in history {
        let mut assigned = false;
        let mut best_centroid = 0usize;
        let mut best_sim = threshold;

        for (cid, (centroid, count)) in &centroids {
            let sim = cosine_similarity(vec, centroid);
            if sim > best_sim {
                best_sim = sim;
                best_centroid = *cid;
                assigned = true;
            }
        }

        if assigned {
            // Update centroid (online average)
            let entry = centroids.get_mut(&best_centroid).unwrap();
            let new_count = entry.1 + 1;
            let alpha = 1.0 / new_count as f64;
            for (c, v) in entry.0.iter_mut().zip(vec.iter()) {
                *c += alpha * (v - *c);
            }
            entry.1 = new_count;
        } else {
            // New cluster
            let new_id = centroids.len();
            centroids.insert(new_id, (vec.clone(), 1));
        }
    }

    let cluster_count = centroids.len();

    // Intra-cluster coherence
    let mut intra_sum = 0.0;
    let mut intra_count = 0usize;
    for (cid, (centroid, _)) in &centroids {
        for vec in history {
            let sim = cosine_similarity(vec, centroid);
            intra_sum += sim;
            intra_count += 1;
        }
    }
    let intra_coherence = if intra_count > 0 { intra_sum / intra_count as f64 } else { 0.0 };

    // Inter-cluster separation
    let centroids_list: Vec<_> = centroids.values().map(|(c, _)| c.clone()).collect();
    let mut inter_sum = 0.0;
    let mut inter_count = 0usize;
    for i in 0..centroids_list.len() {
        for j in (i + 1)..centroids_list.len() {
            let sim = cosine_similarity(&centroids_list[i], &centroids_list[j]);
            inter_sum += 1.0 - sim; // dissimilarity
            inter_count += 1;
        }
    }
    let inter_separation = if inter_count > 0 { inter_sum / inter_count as f64 } else { 0.0 };
    let separation_ratio = if inter_separation > 0.0 { intra_coherence / inter_separation } else { intra_coherence };

    ClusterReport {
        cluster_count,
        intra_coherence,
        inter_separation,
        separation_ratio,
        total_observations: n,
    }
}

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt().max(f64::EPSILON);
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt().max(f64::EPSILON);
    dot / (norm_a * norm_b)
}

/// One-shot log to stderr (avoids log crate dependency).
fn log_once(msg: &str) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static LOGGED: AtomicBool = AtomicBool::new(false);
    if !LOGGED.swap(true, Ordering::Relaxed) {
        eprintln!("[llm_bridge] {}", msg);
    }
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_observer_creation() {
        let config = LLMConfig::default();
        let observer = LLMObserver::new(config);
        assert!(observer.last_latent().is_none());
        assert!(observer.latent_history().is_empty());
    }

    #[test]
    fn test_mock_forward_consistency() {
        let config = LLMConfig::default();
        let mut observer = LLMObserver::new(config);

        let l1 = observer.observe("Hello world").unwrap();
        let l2 = observer.observe("Hello world").unwrap();

        // Same text → same latent (deterministic mock)
        assert_eq!(l1.len(), l2.len());
        // Vectors should be identical for same input
        let sim = cosine_similarity(&l1, &l2);
        assert!((sim - 1.0).abs() < 1e-6, "Deterministic mock failed: sim={}", sim);
    }

    #[test]
    fn test_mock_forward_different_texts() {
        let config = LLMConfig::default();
        let mut observer = LLMObserver::new(config);

        let l1 = observer.observe("The cat sat on the mat").unwrap();
        let l2 = observer.observe("Quantum mechanics is fascinating").unwrap();

        let sim = cosine_similarity(&l1, &l2);
        assert!(sim < 0.99, "Different texts should produce different latents");
    }

    #[test]
    fn test_cluster_analysis() {
        let config = LLMConfig {
            d_model: 16,
            ..LLMConfig::default()
        };
        let mut observer = LLMObserver::new(config);

        // Generate observations from 3 "topics"
        let topics = vec![
            "machine learning", "neural networks", "deep learning",
            "quantum physics", "quantum computing", "particle physics",
            "classical music", "jazz improvisation", "orchestral composition",
        ];

        for _ in 0..5 {
            for topic in &topics {
                let _ = observer.observe(topic);
            }
        }

        let report = analyze_clusters(&observer);
        assert!(report.cluster_count >= 1, "Should detect at least 1 cluster, got {}", report.cluster_count);
        assert!(report.intra_coherence > 0.0);
        assert!(report.total_observations == 45);
    }

    #[test]
    fn test_default_config_sanity() {
        let config = LLMConfig::default();
        assert_eq!(config.d_model, 2048);
        assert_eq!(config.vocab_size, 32000);
        assert_eq!(config.max_seq_len, 2048);
    }
}
