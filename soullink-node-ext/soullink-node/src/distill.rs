//! Knowledge Distillation engine — arxiv 2311.10642 pattern.
//!
//! Trains a shallow feed-forward "student" network to mimic a larger "teacher"
//! model by matching its output distribution via soft-target KL divergence.
//!
//! # SoulLink use cases
//!
//! 1. **ssm_cortex compression** — distill MambaModel predictions into a tiny
//!    FF network for real-time brain modulation without the full SSM overhead.
//! 2. **meta_cortex acceleration** — replace expensive counterfactual reasoning
//!    lookups with a distilled FF that predicts intervention outcomes.
//! 3. **Hot-path predictor** — any high-frequency prediction loop where the
//!    teacher is too slow but offline recording is feasible.
//!
//! # Algorithm (Hinton et al. 2015, plus paper 2311.10642 adaptation)
//!
//! 1. Record (input, teacher_output) pairs during normal operation.
//! 2. Train student via:
//!    - Soft loss: KL(softmax(teacher/T) || softmax(student/T)) × T²
//!    - Hard loss (optional): MSE(student, ground_truth) × α
//! 3. Replace teacher with student at inference time.
//!
//! The student is a single-hidden-layer FF (configurable depth/width) that
//! processes concatenated feature vectors — matching the ALR (Attention Layer
//! Replacement) pattern from the paper but adapted for scalar/vector regression
//! instead of token-level BLEU.

use ndarray::{Array1, Array2};
use rand::seq::SliceRandom;
use rand::rng;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

// ── Configuration ──────────────────────────────────────────────────────────

/// Distillation hyperparameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillConfig {
    /// Temperature for soft-target softening (higher = softer).
    pub temperature: f64,
    /// Weight of hard-target loss vs soft loss in [0, 1].
    pub alpha: f64,
    /// Learning rate.
    pub lr: f64,
    /// Maximum training epochs per distillation cycle.
    pub max_epochs: usize,
    /// Batch size.
    pub batch_size: usize,
    /// Number of recorded samples before triggering distillation.
    pub trigger_samples: usize,
    /// Maximum ring buffer capacity for recorded samples.
    pub buffer_capacity: usize,
    /// Hidden layer size for student FF.
    pub hidden_dim: usize,
    /// Number of hidden layers (1 = shallow, matching paper).
    pub hidden_layers: usize,
    /// L2 regularization strength.
    pub l2_reg: f64,
    /// Validation split fraction.
    pub val_split: f64,
    /// If true, distill only when validation loss plateaus/stops improving.
    pub early_stop_patience: usize,
}

impl Default for DistillConfig {
    fn default() -> Self {
        Self {
            temperature: 4.0,
            alpha: 0.1,
            lr: 0.001,
            max_epochs: 200,
            batch_size: 64,
            trigger_samples: 1000,
            buffer_capacity: 10000,
            hidden_dim: 32,
            hidden_layers: 1,
            l2_reg: 1e-5,
            val_split: 0.2,
            early_stop_patience: 20,
        }
    }
}

// ── Recorded sample ────────────────────────────────────────────────────────

/// A single (input, teacher_output) pair for distillation training.
#[derive(Debug, Clone)]
pub struct DistillSample {
    /// Input features as a flat vector.
    pub input: Array1<f64>,
    /// Teacher model's raw output (before any activation).
    pub teacher_logits: Array1<f64>,
    /// Optional ground truth / hard target (if available).
    pub hard_target: Option<Array1<f64>>,
    /// Sample weight (e.g. recency or importance).
    pub weight: f64,
}

// ── Student model ──────────────────────────────────────────────────────────

/// A shallow multi-layer feed-forward student network.
///
/// Matches the ALR pattern from the paper: input → hidden(ReLU) → output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudentFF {
    /// Weight matrices (input_dim → hidden then hidden → hidden … → output_dim).
    pub weights: Vec<Array2<f64>>,
    /// Bias vectors, one per layer.
    pub biases: Vec<Array1<f64>>,
    /// Architecture: [input_dim, hidden, ..., hidden, output_dim].
    pub layer_sizes: Vec<usize>,
}

impl StudentFF {
    /// Initialize a new student with given layer sizes.
    /// Example: `new(&[44, 32, 10])` = 44→32→10 with one hidden layer.
    pub fn new(layer_sizes: &[usize]) -> Self {
        assert!(layer_sizes.len() >= 2, "need at least input and output layers");
        let mut rng = rand::rng();
        let mut weights = Vec::with_capacity(layer_sizes.len() - 1);
        let mut biases = Vec::with_capacity(layer_sizes.len() - 1);

        for i in 0..layer_sizes.len() - 1 {
            let fan_in = layer_sizes[i];
            let fan_out = layer_sizes[i + 1];
            // Xavier / Glorot uniform init
            let limit = (6.0_f64 / (fan_in + fan_out) as f64).sqrt();
            let w = Array2::from_shape_fn((fan_out, fan_in), |_| {
                rand::Rng::random_range(&mut rng, -limit..limit)
            });
            let b = Array1::zeros(fan_out);
            weights.push(w);
            biases.push(b);
        }

        Self { weights, biases, layer_sizes: layer_sizes.to_vec() }
    }

    /// Forward pass returning raw logits (no final softmax/normalization).
    pub fn forward(&self, x: &Array1<f64>) -> Array1<f64> {
        let mut h = x.clone();
        let n_layers = self.weights.len();

        for l in 0..n_layers {
            h = self.weights[l].dot(&h) + &self.biases[l];
            // ReLU on all except the last layer
            if l < n_layers - 1 {
                h.mapv_inplace(|v| v.max(0.0));
            }
        }
        h
    }

    /// Input dimension.
    pub fn input_dim(&self) -> usize {
        self.layer_sizes[0]
    }

    /// Output dimension.
    pub fn output_dim(&self) -> usize {
        *self.layer_sizes.last().unwrap()
    }

    /// Number of trainable parameters.
    pub fn num_params(&self) -> usize {
        self.weights.iter().map(|w| w.len()).sum::<usize>()
            + self.biases.iter().map(|b| b.len()).sum::<usize>()
    }
}

// ── Softmax helper ─────────────────────────────────────────────────────────

/// Softmax with temperature scaling.
fn softmax(logits: &Array1<f64>, temp: f64) -> Array1<f64> {
    let scaled = logits / temp;
    let max_val = scaled.fold(f64::NEG_INFINITY, |a, &b| a.max(b));
    let exp_vals = scaled.mapv(|v| ((v - max_val) as f64).exp());
    let sum = exp_vals.sum();
    if sum == 0.0 {
        return Array1::ones(logits.len()) / logits.len() as f64;
    }
    exp_vals / sum
}

/// KL divergence: KL(p || q) = Σ p_i log(p_i / q_i).
fn kl_divergence(p: &Array1<f64>, q: &Array1<f64>) -> f64 {
    let eps = 1e-10;
    p.iter()
        .zip(q.iter())
        .map(|(&pi, &qi)| {
            let pi_safe = pi.max(eps);
            let qi_safe = qi.max(eps);
            pi_safe * (pi_safe / qi_safe).ln()
        })
        .sum()
}

// ── Distillation engine ────────────────────────────────────────────────────

/// Knowledge distillation engine.
///
/// Records teacher outputs during operation, then periodically trains a
/// small student FF to match the teacher distribution.
pub struct DistillationEngine {
    /// Configuration.
    pub config: DistillConfig,
    /// Student model (None until first distillation).
    pub student: Option<StudentFF>,
    /// Ring buffer of recorded samples for training.
    pub buffer: VecDeque<DistillSample>,
    /// Number of distillation cycles completed.
    pub cycles: u64,
    /// Last validation loss (None if never distilled).
    pub last_val_loss: Option<f64>,
    /// Whether the student is currently active (replacing teacher).
    pub student_active: bool,
}

impl DistillationEngine {
    /// Create a new distillation engine.
    pub fn new(config: DistillConfig) -> Self {
        Self {
            config,
            student: None,
            buffer: VecDeque::new(),
            cycles: 0,
            last_val_loss: None,
            student_active: false,
        }
    }

    /// Record a teacher prediction sample.
    ///
    /// Called during normal operation — cheap, just appends to ring buffer.
    pub fn record(
        &mut self,
        input: Array1<f64>,
        teacher_logits: Array1<f64>,
        hard_target: Option<Array1<f64>>,
    ) {
        let sample = DistillSample {
            input,
            teacher_logits,
            hard_target,
            weight: 1.0,
        };

        if self.buffer.len() >= self.config.buffer_capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(sample);
    }

    /// Check whether enough samples have accumulated to trigger distillation.
    pub fn should_distill(&self) -> bool {
        self.buffer.len() >= self.config.trigger_samples
    }

    /// Run a full distillation cycle: train student from recorded samples.
    ///
    /// Returns the final validation loss.
    pub fn distill(&mut self) -> Option<f64> {
        let min_samples = self.config.trigger_samples.min(100);
        if self.buffer.len() < min_samples {
            return None; // need a minimum to train
        }

        let n_samples = self.buffer.len();
        let val_n = (n_samples as f64 * self.config.val_split).ceil() as usize;
        let val_n = val_n.min(n_samples / 4); // cap at 25% for validation

        let mut indices: Vec<usize> = (0..n_samples).collect();
        indices.shuffle(&mut rng());

        let train_indices: Vec<usize> = indices[val_n..].to_vec();
        let val_indices: Vec<usize> = indices[..val_n].to_vec();

        let input_dim = self.buffer[0].input.len();
        let output_dim = self.buffer[0].teacher_logits.len();

        // Build layer sizes: input → hidden(s) → output
        let mut layer_sizes = vec![input_dim];
        for _ in 0..self.config.hidden_layers {
            layer_sizes.push(self.config.hidden_dim);
        }
        layer_sizes.push(output_dim);

        let mut student = StudentFF::new(&layer_sizes);
        let temp = self.config.temperature;

        let mut best_val_loss = f64::MAX;
        let mut best_student = student.clone();
        let mut patience_counter = 0;

        for _epoch in 0..self.config.max_epochs {
            // Shuffle training indices
            let mut epoch_indices = train_indices.clone();
            epoch_indices.shuffle(&mut rng());

            // Mini-batch training
            for batch_start in (0..epoch_indices.len()).step_by(self.config.batch_size) {
                let batch_end = (batch_start + self.config.batch_size).min(epoch_indices.len());
                let batch: Vec<usize> = epoch_indices[batch_start..batch_end].to_vec();

                let mut grad_weights: Vec<Array2<f64>> = student
                    .weights
                    .iter()
                    .map(|w| Array2::zeros(w.dim()))
                    .collect();
                let mut grad_biases: Vec<Array1<f64>> = student
                    .biases
                    .iter()
                    .map(|b| Array1::zeros(b.len()))
                    .collect();

                for &idx in &batch {
                    let sample = &self.buffer[idx];

                    // Forward pass through student
                    let mut activations = vec![sample.input.clone()];
                    let mut pre_acts = Vec::new();

                    for l in 0..student.weights.len() {
                        let pre = student.weights[l].dot(activations.last().unwrap())
                            + &student.biases[l];
                        let post = if l < student.weights.len() - 1 {
                            pre.mapv(|v| v.max(0.0))
                        } else {
                            pre.clone()
                        };
                        pre_acts.push(pre);
                        activations.push(post);
                    }

                    let student_logits = activations.last().unwrap();

                    // Soft loss (KL divergence on soft targets)
                    let teacher_soft = softmax(&sample.teacher_logits, temp);
                    let student_soft = softmax(student_logits, temp);
                    let soft_loss = kl_divergence(&teacher_soft, &student_soft) * temp * temp;

                    // Hard loss (MSE against ground truth, if available)
                    let hard_loss = if let Some(ref target) = sample.hard_target {
                        let diff = student_logits - target;
                        diff.mapv(|v| v * v).sum() / output_dim as f64
                    } else {
                        0.0
                    };

                    let _total_loss =
                        (1.0 - self.config.alpha) * soft_loss + self.config.alpha * hard_loss;

                    // Backprop through output layer
                    let grad_soft: Array1<f64> = {
                        let teacher_s = teacher_soft.clone();
                        let student_s = student_soft.clone();
                        let eps = 1e-10;
                        let mut g = Array1::zeros(output_dim);
                        for i in 0..output_dim {
                            let si = student_s[i].max(eps);
                            let ti = teacher_s[i].max(eps);
                            let d_kl = -ti / si; // dKL/ds_i
                            // d(softmax)/d(logit) = softmax_i * (delta_ij - softmax_j)
                            for j in 0..output_dim {
                                let kronecker = if i == j { 1.0 } else { 0.0 };
                                g[j] += d_kl * si * (kronecker - student_s[j]) * temp * temp
                                    * (1.0 - self.config.alpha);
                            }
                        }
                        g
                    };

                    let grad_hard: Array1<f64> = if let Some(ref target) = sample.hard_target {
                        (student_logits - target) * (2.0 * self.config.alpha / output_dim as f64)
                    } else {
                        Array1::zeros(output_dim)
                    };

                    let mut delta = &grad_soft + &grad_hard;

                    // Backprop through layers
                    let last_idx = student.weights.len() - 1;
                    // Output layer gradients
                    {
                        let a_in = &activations[last_idx];
                        for i in 0..output_dim {
                            for j in 0..a_in.len() {
                                grad_weights[last_idx][(i, j)] += delta[i] * a_in[j];
                            }
                            grad_biases[last_idx][i] += delta[i];
                        }
                        // Propagate delta to previous layer
                        delta = student.weights[last_idx].t().dot(&delta);
                        // Apply ReLU derivative for hidden layer
                        let pre = &pre_acts[last_idx - 1];
                        delta = delta * &pre.mapv(|v| if v > 0.0 { 1.0 } else { 0.0 });
                    }

                    // Hidden layer gradients (for single-layer; extend for deeper)
                    for l in (0..student.weights.len() - 1).rev() {
                        let a_in = &activations[l];
                        let out_dim = student.layer_sizes[l + 1];
                        let in_dim = a_in.len();
                        for i in 0..out_dim {
                            for j in 0..in_dim {
                                grad_weights[l][(i, j)] += delta[i] * a_in[j];
                            }
                            grad_biases[l][i] += delta[i];
                        }
                        if l > 0 {
                            delta = student.weights[l].t().dot(&delta);
                            let pre = &pre_acts[l - 1];
                            delta = delta * &pre.mapv(|v| if v > 0.0 { 1.0 } else { 0.0 });
                        }
                    }
                }

                // SGD update with L2 regularization
                let lr = self.config.lr;
                for l in 0..student.weights.len() {
                    let w_reg = &student.weights[l] * self.config.l2_reg;
                    student.weights[l] =
                        &student.weights[l] - &(&grad_weights[l] * (lr / batch.len() as f64)) -
                        &(w_reg * lr);
                    student.biases[l] =
                        &student.biases[l] - &(&grad_biases[l] * (lr / batch.len() as f64));
                }
            }

            // Validation
            let val_loss = compute_val_loss(&student, &self.buffer, &val_indices, temp, self.config.alpha);
            if val_loss < best_val_loss {
                best_val_loss = val_loss;
                best_student = student.clone();
                patience_counter = 0;
            } else {
                patience_counter += 1;
                if patience_counter >= self.config.early_stop_patience {
                    break;
                }
            }
        }

        self.student = Some(best_student);
        self.cycles += 1;
        self.last_val_loss = Some(best_val_loss);
        self.student_active = true;
        Some(best_val_loss)
    }

    /// Predict using the distilled student (or fall back to teacher).
    /// Returns None if no student has been trained yet.
    pub fn predict(&self, input: &Array1<f64>) -> Option<Array1<f64>> {
        self.student.as_ref().map(|s| s.forward(input))
    }

    /// Deactivate student — revert to teacher for predictions.
    pub fn deactivate_student(&mut self) {
        self.student_active = false;
    }

    /// Activate student (after it's been trained).
    pub fn activate_student(&mut self) {
        if self.student.is_some() {
            self.student_active = true;
        }
    }

    /// Clear the sample buffer (e.g., after distillation or model drift).
    pub fn clear_buffer(&mut self) {
        self.buffer.clear();
    }

    /// Get statistics about the distillation state.
    pub fn stats(&self) -> DistillStats {
        DistillStats {
            buffer_len: self.buffer.len(),
            cycles: self.cycles,
            student_active: self.student_active,
            last_val_loss: self.last_val_loss,
            student_params: self.student.as_ref().map(|s| s.num_params()),
        }
    }
}

/// Compute mean validation loss across validation indices.
fn compute_val_loss(
    student: &StudentFF,
    buffer: &VecDeque<DistillSample>,
    val_indices: &[usize],
    temp: f64,
    alpha: f64,
) -> f64 {
    let mut total = 0.0;
    for &idx in val_indices {
        let sample = &buffer[idx];
        let student_logits = student.forward(&sample.input);
        let teacher_soft = softmax(&sample.teacher_logits, temp);
        let student_soft = softmax(&student_logits, temp);
        let soft_loss = kl_divergence(&teacher_soft, &student_soft) * temp * temp;
        let hard_loss = if let Some(ref target) = sample.hard_target {
            let diff = &student_logits - target;
            diff.mapv(|v| v * v).sum() / sample.teacher_logits.len() as f64
        } else {
            0.0
        };
        total += (1.0 - alpha) * soft_loss + alpha * hard_loss;
    }
    total / val_indices.len() as f64
}

// ── Statistics ─────────────────────────────────────────────────────────────

/// Read-only snapshot of distillation engine state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillStats {
    pub buffer_len: usize,
    pub cycles: u64,
    pub student_active: bool,
    pub last_val_loss: Option<f64>,
    pub student_params: Option<usize>,
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_student_forward_shape() {
        let student = StudentFF::new(&[10, 32, 5]);
        let input = Array1::linspace(0.0, 1.0, 10);
        let output = student.forward(&input);
        assert_eq!(output.len(), 5);
    }

    #[test]
    fn test_student_params_count() {
        let student = StudentFF::new(&[10, 32, 5]);
        let n = student.num_params();
        // weights: 32*10 + 5*32 = 320 + 160 = 480
        // biases: 32 + 5 = 37
        // total = 517
        assert_eq!(n, 517);
    }

    #[test]
    fn test_softmax_is_distribution() {
        let logits = Array1::from_vec(vec![1.0, 2.0, 3.0]);
        let probs = softmax(&logits, 1.0);
        assert!((probs.sum() - 1.0).abs() < 1e-10);
        assert!(probs[2] > probs[1]);
        assert!(probs[1] > probs[0]);
    }

    #[test]
    fn test_softmax_temperature() {
        let logits = Array1::from_vec(vec![1.0, 3.0]);
        let sharp = softmax(&logits, 1.0); // T=1
        let soft = softmax(&logits, 10.0); // T=10 — more uniform
        assert!(sharp[1] > soft[1]);
    }

    #[test]
    fn test_kl_divergence_self_is_zero() {
        let p = softmax(&Array1::from_vec(vec![1.0, 2.0]), 1.0);
        let kl = kl_divergence(&p, &p);
        assert!(kl < 1e-8);
    }

    #[test]
    fn test_record_and_distill() {
        let config = DistillConfig {
            trigger_samples: 10,
            batch_size: 4,
            max_epochs: 5,
            ..Default::default()
        };

        let mut engine = DistillationEngine::new(config);

        // Record synthetic samples: input[0..2] → identity output[0..1]
        for i in 0..20 {
            let input = Array1::from_vec(vec![i as f64, (i % 3) as f64]);
            let teacher = Array1::from_vec(vec![i as f64 * 0.5, (i % 3) as f64 * 0.5]);
            engine.record(input.clone(), teacher, None);
        }

        assert!(engine.should_distill());
        let val_loss = engine.distill().expect("distillation failed");
        assert!(val_loss.is_finite());

        // Student should produce something reasonable
        let pred = engine
            .predict(&Array1::from_vec(vec![5.0, 1.0]))
            .expect("no student");
        assert_eq!(pred.len(), 2);
    }

    #[test]
    fn test_distill_stats() {
        let config = DistillConfig::default();
        let engine = DistillationEngine::new(config);
        let stats = engine.stats();
        assert_eq!(stats.buffer_len, 0);
        assert_eq!(stats.cycles, 0);
        assert!(!stats.student_active);
    }
}
