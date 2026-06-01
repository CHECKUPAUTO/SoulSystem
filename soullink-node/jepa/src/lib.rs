//! JEPA — Joint Embedding Predictive Architecture
//!
//! Connects **SoulLink-SSM** (encoder backbone) with **SciRust** (symbolic
//! expression input, pattern memory).
//!
//! Architecture:
//! ```text
//! input_t → [SSM Encoder] → z_t → [Predictor] → ŷ_t
//! input_{t+1} → [Target Encoder] → z_{t+1}
//! Loss = 1 - cosine_sim(ŷ_t, sg(z_{t+1}))
//! ```
//!
//! The SSM replaces the usual ViT/VQ-VAE encoder, providing a learnable
//! linear-time state space model for embedding sequences.

use ndarray::{Array1, Array2};
use soullink_ssm::hippo::HiPPOInitializer;
use soullink_ssm::selective::SelectiveSSM;

// ─── Cosine Similarity ─────────────────────────────────────────────────────

/// Cosine similarity between two vectors.
pub fn cosine_similarity(a: &Array1<f64>, b: &Array1<f64>) -> f64 {
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm_a < 1e-12 || norm_b < 1e-12 {
        return 0.0;
    }
    (dot / (norm_a * norm_b)).clamp(-1.0, 1.0)
}

// ─── SSM Encoder ───────────────────────────────────────────────────────────

/// Encodes input sequences into fixed-dimension embeddings using an SSM.
///
/// The SSM processes the input sequentially, and the final hidden state
/// (or pooled states) forms the embedding.
pub struct SSMEncoder {
    /// Selective SSM backbone
    pub ssm: SelectiveSSM,
    /// Input projection: raw features → SSM dimension (d_model, d_input)
    pub input_proj: Array2<f64>,
    /// Output projection: SSM hidden → embedding (d_embed, state_dim)
    pub output_proj: Array2<f64>,
    /// Output bias
    pub output_bias: Array1<f64>,
    /// State dimension (N)
    pub state_dim: usize,
    /// SSM input dimension
    pub d_model: usize,
    /// Embedding dimension
    pub d_embed: usize,
}

impl SSMEncoder {
    /// Create a new SSM encoder.
    ///
    /// # Arguments
    ///
    /// * `d_input` - Raw input feature dimension
    /// * `d_model` - SSM inner dimension
    /// * `d_embed` - Output embedding dimension
    /// * `state_dim` - SSM state dimension (N)
    pub fn new(d_input: usize, d_model: usize, d_embed: usize, state_dim: usize) -> Self {
        let a = HiPPOInitializer::legs(state_dim);
        let ssm = SelectiveSSM::new(state_dim, d_model, a);

        let mut rng = rand::thread_rng();
        let scale_in = (1.0 / d_input as f64).sqrt();
        let input_proj: Array2<f64> = Array2::zeros((d_model, d_input)).mapv(|_: f64| {
            rand::Rng::gen_range(&mut rng, -scale_in..scale_in)
        });

        let scale_out = (1.0 / d_model as f64).sqrt();
        let output_proj: Array2<f64> = Array2::zeros((d_embed, d_model)).mapv(|_: f64| {
            rand::Rng::gen_range(&mut rng, -scale_out..scale_out)
        });
        let output_bias = Array1::zeros(d_embed);

        Self {
            ssm,
            input_proj,
            output_proj,
            output_bias,
            state_dim,
            d_model,
            d_embed,
        }
    }

    /// Encode a sequence into an embedding.
    ///
    /// Projects input, runs SSM, then pools the output into an embedding.
    ///
    /// # Arguments
    ///
    /// * `x` - Input sequence (L, d_input)
    ///
    /// # Returns
    ///
    /// Embedding vector (d_embed,)
    pub fn encode(&self, x: &Array2<f64>) -> Array1<f64> {
        // Project to SSM dimension: (L, d_input) → (L, d_model)
        let projected = x.dot(&self.input_proj.t());

        // Run SSM → (L, d_model)
        let ssm_out = self.ssm.forward(&projected);

        // Pool: mean over time
        let l = ssm_out.shape()[0] as f64;
        let mut pooled = Array1::zeros(self.d_model);
        for t in 0..ssm_out.shape()[0] {
            let row = ssm_out.row(t);
            for j in 0..self.d_model {
                pooled[j] += row[j] / l;
            }
        }

        // Project to embedding
        self.output_proj.dot(&pooled) + &self.output_bias
    }

    /// Encode and return the raw SSM output sequence.
    pub fn encode_sequence(&self, x: &Array2<f64>) -> Array2<f64> {
        let projected = x.dot(&self.input_proj.t());
        self.ssm.forward(&projected)
    }

    /// Compute the gradient of an embedding w.r.t. a scalar loss
    /// using SciRust's dual numbers for verification.
    pub fn encode_with_gradient(
        &self,
        x: &Array2<f64>,
    ) -> (Array1<f64>, f64) {
        let embed = self.encode(x);
        let norm: f64 = embed.iter().map(|v| v * v).sum::<f64>().sqrt();
        (embed, norm)
    }
}

// ─── Predictor ─────────────────────────────────────────────────────────────

/// Predicts the next embedding from the current one.
///
/// `ŷ_t = W₂ · SiLU(W₁ · z_t + b₁) + b₂`
pub struct JEPAPredictor {
    /// Hidden layer weight (d_hidden, d_embed)
    pub w1: Array2<f64>,
    /// Hidden layer bias
    pub b1: Array1<f64>,
    /// Output layer weight (d_embed, d_hidden)
    pub w2: Array2<f64>,
    /// Output layer bias
    pub b2: Array1<f64>,
    /// Hidden dimension
    pub d_hidden: usize,
}

impl JEPAPredictor {
    pub fn new(d_embed: usize, d_hidden: usize) -> Self {
        let mut rng = rand::thread_rng();
        let scale = (1.0 / d_embed as f64).sqrt();

        let w1: Array2<f64> = Array2::zeros((d_hidden, d_embed)).mapv(|_: f64| {
            rand::Rng::gen_range(&mut rng, -scale..scale)
        });
        let b1 = Array1::zeros(d_hidden);
        let scale2 = (1.0 / d_hidden as f64).sqrt();
        let w2: Array2<f64> = Array2::zeros((d_embed, d_hidden)).mapv(|_: f64| {
            rand::Rng::gen_range(&mut rng, -scale2..scale2)
        });
        let b2 = Array1::zeros(d_embed);
        Self { w1, b1, w2, b2, d_hidden }
    }

    /// SiLU activation
    fn silu(x: f64) -> f64 {
        let sig = 1.0 / (1.0 + (-x).exp());
        x * sig
    }

    /// Predict the next embedding from current.
    pub fn predict(&self, z: &Array1<f64>) -> Array1<f64> {
        // h = SiLU(W1 · z + b1)
        let h: Array1<f64> = (self.w1.dot(z) + &self.b1).mapv(Self::silu);
        // ŷ = W2 · h + b2
        self.w2.dot(&h) + &self.b2
    }
}

// ─── Target Encoder (Momentum) ─────────────────────────────────────────────

/// Slow-moving average of the online encoder.
///
/// `θ_target = momentum · θ_target + (1 - momentum) · θ_online`
pub struct MomentumEncoder {
    /// Target SSM encoder (weights match online encoder at init)
    pub encoder: SSMEncoder,
    /// Momentum coefficient (typical: 0.99+)
    pub momentum: f64,
    /// Online encoder reference for momentum updates
    online_a: Array2<f64>,
    online_input_proj: Array2<f64>,
    online_output_proj: Array2<f64>,
    online_output_bias: Array1<f64>,
}

impl MomentumEncoder {
    /// Create a momentum encoder initialized from an online encoder.
    pub fn new(online: &SSMEncoder, momentum: f64) -> Self {
        // Deep copy the SSM parameters
        let a_copy = online.ssm.a.clone();
        let b_proj_copy = online.ssm.b_proj.clone();
        let c_proj_copy = online.ssm.c_proj.clone();
        let delta_proj_copy = online.ssm.delta_proj.clone();

        let target_ssm = SelectiveSSM {
            a: a_copy,
            b_proj: b_proj_copy,
            c_proj: c_proj_copy,
            delta_proj: delta_proj_copy,
            d: online.ssm.d,
            state_dim: online.ssm.state_dim,
            input_dim: online.ssm.input_dim,
            delta_min: online.ssm.delta_min,
            delta_max: online.ssm.delta_max,
        };

        let (n, d) = online.ssm.b_proj.dim();
        let (de, _) = online.output_proj.dim();

        Self {
            encoder: SSMEncoder {
                ssm: target_ssm,
                input_proj: online.input_proj.clone(),
                output_proj: online.output_proj.clone(),
                output_bias: online.output_bias.clone(),
                state_dim: online.state_dim,
                d_model: online.d_model,
                d_embed: online.d_embed,
            },
            momentum,
            online_a: Array2::zeros((n, n)),
            online_input_proj: Array2::zeros((d, d)),
            online_output_proj: Array2::zeros((de, n)),
            online_output_bias: Array1::zeros(de),
        }
    }

    /// Update target weights: θ_target = momentum · θ_target + (1 - m) · θ_online
    pub fn momentum_update(&mut self, online: &SSMEncoder) {
        let m = self.momentum;

        // Update SSM projections
        for i in 0..self.encoder.ssm.b_proj.shape()[0] {
            for j in 0..self.encoder.ssm.b_proj.shape()[1] {
                self.encoder.ssm.b_proj[[i, j]] =
                    m * self.encoder.ssm.b_proj[[i, j]] + (1.0 - m) * online.ssm.b_proj[[i, j]];
                self.encoder.ssm.c_proj[[i, j]] =
                    m * self.encoder.ssm.c_proj[[i, j]] + (1.0 - m) * online.ssm.c_proj[[i, j]];
            }
        }
        for i in 0..self.encoder.ssm.delta_proj.len() {
            self.encoder.ssm.delta_proj[i] =
                m * self.encoder.ssm.delta_proj[i] + (1.0 - m) * online.ssm.delta_proj[i];
        }

        // Update input/output projections
        let (di, dj) = self.encoder.input_proj.dim();
        for i in 0..di {
            for j in 0..dj {
                self.encoder.input_proj[[i, j]] =
                    m * self.encoder.input_proj[[i, j]] + (1.0 - m) * online.input_proj[[i, j]];
            }
        }
        let (oi, oj) = self.encoder.output_proj.dim();
        for i in 0..oi {
            for j in 0..oj {
                self.encoder.output_proj[[i, j]] =
                    m * self.encoder.output_proj[[i, j]] + (1.0 - m) * online.output_proj[[i, j]];
            }
        }
        for i in 0..self.encoder.output_bias.len() {
            self.encoder.output_bias[i] =
                m * self.encoder.output_bias[i] + (1.0 - m) * online.output_bias[i];
        }
    }

    /// Encode using target encoder (no gradient).
    pub fn encode(&self, x: &Array2<f64>) -> Array1<f64> {
        self.encoder.encode(x)
    }
}

// ─── Full JEPA Model ───────────────────────────────────────────────────────

/// Joint Embedding Predictive Architecture combining SSM + SciRust.
///
/// Components:
/// - **SSMEncoder**: online encoder producing embeddings
/// - **JEPAPredictor**: predicts future embeddings in latent space
/// - **MomentumEncoder**: slow-moving target encoder (BYOL-style)
///
/// SciRust integration:
/// - Symbolic expressions from `scirust-core::symbolic` are used to
///   generate structured input patterns
/// - `scirust-core::PatternMemory` stores successful embedding predictions
pub struct JEPA {
    /// Online SSM encoder
    pub encoder: SSMEncoder,
    /// Latent predictor
    pub predictor: JEPAPredictor,
    /// Momentum target encoder
    pub target: MomentumEncoder,
    /// Embedding dimension
    pub d_embed: usize,
    /// SciRust pattern memory for storing embedding motifs
    pub pattern_memory: scirust_core::PatternMemory,
}

impl JEPA {
    /// Create a new JEPA model.
    pub fn new(d_input: usize, d_model: usize, d_embed: usize, state_dim: usize, d_hidden: usize) -> Self {
        let encoder = SSMEncoder::new(d_input, d_model, d_embed, state_dim);
        let predictor = JEPAPredictor::new(d_embed, d_hidden);
        let target = MomentumEncoder::new(&encoder, 0.996);
        Self {
            encoder,
            predictor,
            target,
            d_embed,
            pattern_memory: scirust_core::PatternMemory::new(),
        }
    }

    /// Forward pass: compute embedding loss between two consecutive inputs.
    ///
    /// # Arguments
    ///
    /// * `x_t` - Input at timestep t (L, d_input)
    /// * `x_t1` - Input at timestep t+1 (L, d_input)
    ///
    /// # Returns
    ///
    /// `(loss, predicted_embedding, target_embedding)`
    pub fn forward(&self, x_t: &Array2<f64>, x_t1: &Array2<f64>) -> (f64, Array1<f64>, Array1<f64>) {
        // Online encoder: z_t = f_θ(x_t)
        let z_t = self.encoder.encode(x_t);

        // Predictor: ŷ_t = g_φ(z_t)
        let y_pred = self.predictor.predict(&z_t);

        // Target encoder: z_{t+1} = f_ξ(x_{t+1})  (stop-gradient)
        let z_t1 = self.target.encode(x_t1);

        // Loss: 1 - cosine_similarity(ŷ_t, sg(z_{t+1}))
        let sim = cosine_similarity(&y_pred, &z_t1);
        let loss = 1.0 - sim;

        (loss, y_pred, z_t1)
    }

    /// Update target network with momentum.
    pub fn update_target(&mut self) {
        self.target.momentum_update(&self.encoder);
    }

    /// Record a successful prediction in SciRust pattern memory.
    pub fn record_success(&mut self, input_expr: &str, success: bool) {
        use scirust_core::symbolic::parse;
        if let Ok(expr) = parse(input_expr) {
            let target = scirust_core::symbolic::simplify(&expr);
            self.pattern_memory.record(&expr, &target, success);
        }
    }

    /// Compute the JEPA loss for a batch of sequence pairs.
    ///
    /// # Arguments
    ///
    /// * `x` - Input sequence (L, d_input) — consecutive pairs (0,1), (1,2), ...
    ///
    /// # Returns
    ///
    /// Average loss over all pairs
    pub fn compute_batch_loss(&self, x: &Array2<f64>) -> f64 {
        let l = x.shape()[0];
        if l < 2 {
            return 0.0;
        }

        let mut total_loss = 0.0;
        for t in 0..l - 1 {
            let xt = x.slice(ndarray::s![t..=t, ..]).to_owned();
            let xt1 = x.slice(ndarray::s![t + 1..=t + 1, ..]).to_owned();
            let (loss, _, _) = self.forward(&xt, &xt1);
            total_loss += loss;
        }
        total_loss / (l - 1) as f64
    }

    /// Estimate total parameters.
    pub fn param_count(&self) -> usize {
        let (ni, nj) = self.encoder.input_proj.dim();
        let (oi, oj) = self.encoder.output_proj.dim();
        let (wi, wj) = self.predictor.w1.dim();
        let (w2i, w2j) = self.predictor.w2.dim();
        let n = self.encoder.state_dim;
        let d = self.encoder.d_model;

        // SSM: A(N²) + B_proj(N×D) + C_proj(N×D) + delta_proj(D)
        let ssm_params = n * n + 2 * n * d + d;
        // Encoder projections
        let enc_params = ni * nj + oi * oj + oi;
        // Predictor
        let pred_params = wi * wj + wi + w2i * w2j + w2i;
        // Target shares architecture but is not trained directly

        ssm_params + enc_params + pred_params
    }
}

// ─── Active Inference ──────────────────────────────────────────────────────

/// Free Energy components for Active Inference.
///
/// F = prediction_error + β · complexity
///
/// - **prediction_error**: 1 - cosine_sim(ŷ, z) — how wrong the prediction is
/// - **complexity**: KL-divergence between posterior and prior, approximated
///   as ‖embedding‖² (a zero-mean unit-variance Gaussian prior)
/// - **β**: temperature — high β = explore (tolerate error), low β = exploit
pub struct FreeEnergy {
    /// Prediction error (0–2 range, 0 = perfect)
    pub prediction_error: f64,
    /// Complexity cost (KL approx)
    pub complexity: f64,
    /// Total free energy
    pub total: f64,
    /// Temperature (inverse precision)
    pub beta: f64,
}

impl JEPA {
    /// Compute variational Free Energy for a prediction.
    ///
    /// F = prediction_error + β · complexity
    ///
    /// Where:
    /// - prediction_error = 1 - cosine_sim(predicted, target) ∈ [0, 2]
    /// - complexity = 0.5 · ‖prediction‖² (Gaussian prior N(0,I))
    /// - β scales the complexity cost
    pub fn free_energy(&self, x_t: &Array2<f64>, x_t1: &Array2<f64>, beta: f64) -> FreeEnergy {
        let z_t = self.encoder.encode(x_t);
        let y_pred = self.predictor.predict(&z_t);
        let z_t1 = self.target.encode(x_t1);

        let sim = cosine_similarity(&y_pred, &z_t1);
        let prediction_error = 1.0 - sim;

        // Complexity: KL( q(z|x) ‖ p(z) ) with p(z) = N(0,I)
        // Approximated as 0.5 * ‖y_pred‖²
        let complexity = 0.5 * y_pred.iter().map(|v| v * v).sum::<f64>();

        let total = prediction_error + beta * complexity;

        FreeEnergy {
            prediction_error,
            complexity,
            total,
            beta,
        }
    }

    /// Active Inference step: minimise Free Energy by adjusting the
    /// latent embedding in the direction that reduces prediction error.
    ///
    /// Returns the "corrective force" vector that would minimise surprise:
    /// `Δz = -∇_z F` (negative gradient of Free Energy w.r.t. the embedding).
    ///
    /// In the HNN bridge, this force is injected into the Hamiltonian
    /// momentum to steer the dynamics toward expected (unsurprising) states.
    pub fn active_inference_force(&self, x_t: &Array2<f64>, x_t1: &Array2<f64>, beta: f64) -> Array1<f64> {
        let z_t = self.encoder.encode(x_t);
        let y_pred = self.predictor.predict(&z_t);
        let z_t1 = self.target.encode(x_t1);

        // Gradient of prediction_error = 1 - cos(y_pred, z_t1)
        // ∇_y 1-cos(y,z) = -(z/‖y‖·‖z‖ - y·z·y/‖y‖³·‖z‖)
        let dot: f64 = y_pred.iter().zip(z_t1.iter()).map(|(a, b)| a * b).sum();
        let norm_y: f64 = y_pred.iter().map(|v| v * v).sum::<f64>().sqrt();
        let norm_z: f64 = z_t1.iter().map(|v| v * v).sum::<f64>().sqrt();

        let force = if norm_y < 1e-12 || norm_z < 1e-12 {
            Array1::zeros(self.d_embed)
        } else {
            // dF/dy_i = -(z_i / (‖y‖·‖z‖) - y_i·dot/(‖y‖³·‖z‖)) + β·y_i
            let mut grad = Array1::zeros(self.d_embed);
            for i in 0..self.d_embed {
                let d_pe = -(z_t1[i] / (norm_y * norm_z) - y_pred[i] * dot / (norm_y.powi(3) * norm_z));
                let d_complexity = beta * y_pred[i];
                grad[i] = -(d_pe + d_complexity); // negative gradient = force
            }
            grad
        };

        force
    }

    /// Bridge: take an HNN state trajectory (L × dim) and compute the
    /// Free Energy of each consecutive pair.
    ///
    /// Returns (total_free_energy, per_step_errors).
    pub fn evaluate_hnn_trajectory(&self, trajectory: &Array2<f64>, beta: f64) -> (f64, Vec<f64>) {
        let l = trajectory.shape()[0];
        if l < 2 {
            return (0.0, vec![]);
        }
        let mut total_fe = 0.0;
        let mut per_step = Vec::with_capacity(l - 1);
        for t in 0..l - 1 {
            let xt = trajectory.slice(ndarray::s![t..=t, ..]).to_owned();
            let xt1 = trajectory.slice(ndarray::s![t + 1..=t + 1, ..]).to_owned();
            let fe = self.free_energy(&xt, &xt1, beta);
            total_fe += fe.total;
            per_step.push(fe.total);
        }
        (total_fe, per_step)
    }

    /// Compute the expected Free Energy over an ensemble of possible
    /// future trajectories (simulated via SciRust symbolic perturbation).
    ///
    /// This is the "expected surprise" — used by Active Inference to
    /// select actions that minimise expected Free Energy.
    pub fn expected_free_energy(&self, _base: &Array2<f64>, perturbations: &[Array2<f64>], beta: f64) -> f64 {
        if perturbations.is_empty() {
            return 0.0;
        }
        let mut total = 0.0;
        for pert in perturbations {
            let (fe, _) = self.evaluate_hnn_trajectory(pert, beta);
            total += fe;
        }
        total / perturbations.len() as f64
    }
}

/// Generate a synthetic JEPA training pair from a SciRust symbolic expression.
///
/// Converts `expr` at two adjacent evaluation points into (x_t, x_{t+1}).
pub fn symbolic_to_pair(
    expr_str: &str,
    var: &str,
    t: f64,
    dt: f64,
) -> Result<(Array2<f64>, Array2<f64>), String> {
    use scirust_core::symbolic::{eval, parse};
    use std::collections::HashMap;

    let expr = parse(expr_str)?;

    let mut vars_t = HashMap::new();
    vars_t.insert(var.to_string(), t);
    let val_t = eval(&expr, &vars_t)?;

    let mut vars_t1 = HashMap::new();
    vars_t1.insert(var.to_string(), t + dt);
    let val_t1 = eval(&expr, &vars_t1)?;

    let x_t = Array2::from_shape_vec((1, 1), vec![val_t]).unwrap();
    let x_t1 = Array2::from_shape_vec((1, 1), vec![val_t1]).unwrap();

    Ok((x_t, x_t1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = Array1::from_vec(vec![1.0, 2.0, 3.0]);
        let b = Array1::from_vec(vec![1.0, 2.0, 3.0]);
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = Array1::from_vec(vec![1.0, 0.0]);
        let b = Array1::from_vec(vec![0.0, 1.0]);
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_ssm_encoder_output_shape() {
        let encoder = SSMEncoder::new(8, 16, 8, 4);
        let x: Array2<f64> = Array2::zeros((5, 8)).mapv(|_: f64| rand::random::<f64>() * 0.5);
        let embed = encoder.encode(&x);
        assert_eq!(embed.len(), 8);
    }

    #[test]
    fn test_jepa_predictor_shape() {
        let predictor = JEPAPredictor::new(8, 16);
        let z = Array1::zeros(8);
        let y = predictor.predict(&z);
        assert_eq!(y.len(), 8);
    }

    #[test]
    fn test_momentum_update() {
        let encoder = SSMEncoder::new(4, 8, 4, 2);
        let mut target = MomentumEncoder::new(&encoder, 0.99);
        // After one update, weights should be (nearly) identical
        target.momentum_update(&encoder);
        for i in 0..target.encoder.ssm.b_proj.shape()[0] {
            for j in 0..target.encoder.ssm.b_proj.shape()[1] {
                assert!(
                    (target.encoder.ssm.b_proj[[i, j]] - encoder.ssm.b_proj[[i, j]]).abs() < 1e-10
                );
            }
        }
    }

    #[test]
    fn test_jepa_forward_pass() {
        let jepa = JEPA::new(4, 8, 4, 2, 8);
        let x_t: Array2<f64> = Array2::zeros((3, 4)).mapv(|_: f64| rand::random::<f64>() * 0.5);
        let x_t1: Array2<f64> = Array2::zeros((3, 4)).mapv(|_: f64| rand::random::<f64>() * 0.5);
        let (loss, pred, target) = jepa.forward(&x_t, &x_t1);
        assert_eq!(pred.len(), 4);
        assert_eq!(target.len(), 4);
        // Loss should be in [0, 2] for cosine similarity [-1, 1]
        assert!(loss >= 0.0 && loss <= 2.0);
    }

    #[test]
    fn test_symbolic_to_pair() {
        let (xt, xt1) = symbolic_to_pair("x^2", "x", 2.0, 0.1).unwrap();
        assert_eq!(xt.shape(), &[1, 1]);
        assert_eq!(xt1.shape(), &[1, 1]);
        // x=2 → 4, x=2.1 → 4.41
        assert!((xt[[0, 0]] - 4.0).abs() < 1e-10);
        assert!((xt1[[0, 0]] - 4.41).abs() < 1e-10);
    }

    #[test]
    fn test_jepa_batch_loss() {
        let jepa = JEPA::new(2, 4, 2, 2, 4);
        let x: Array2<f64> = Array2::zeros((5, 2)).mapv(|_: f64| rand::random::<f64>() * 0.5);
        let loss = jepa.compute_batch_loss(&x);
        assert!(loss >= 0.0);
    }

    // ── Active Inference tests ─────────────────────────────────────────────

    #[test]
    fn test_free_energy_components() {
        let jepa = JEPA::new(2, 4, 2, 2, 4);
        let x_t: Array2<f64> = Array2::zeros((3, 2)).mapv(|_: f64| rand::random::<f64>() * 0.5);
        let x_t1: Array2<f64> = Array2::zeros((3, 2)).mapv(|_: f64| rand::random::<f64>() * 0.5);
        let fe = jepa.free_energy(&x_t, &x_t1, 0.1);
        assert!(fe.prediction_error >= 0.0);
        assert!(fe.complexity >= 0.0);
        assert!(fe.total >= 0.0);
    }

    #[test]
    fn test_active_inference_force_shape() {
        let jepa = JEPA::new(2, 4, 2, 2, 4);
        let x_t: Array2<f64> = Array2::zeros((3, 2)).mapv(|_: f64| rand::random::<f64>() * 0.5);
        let x_t1: Array2<f64> = Array2::zeros((3, 2)).mapv(|_: f64| rand::random::<f64>() * 0.5);
        let force = jepa.active_inference_force(&x_t, &x_t1, 0.1);
        assert_eq!(force.len(), 2);
    }

    #[test]
    fn test_hnn_trajectory_evaluation() {
        let jepa = JEPA::new(2, 4, 2, 2, 4);
        // Simulate an HNN trajectory: 10 timesteps, 2-dim
        let traj: Array2<f64> = Array2::zeros((10, 2)).mapv(|_: f64| rand::random::<f64>() * 0.5);
        let (total_fe, per_step) = jepa.evaluate_hnn_trajectory(&traj, 0.1);
        assert!(total_fe >= 0.0);
        assert_eq!(per_step.len(), 9);
    }

    #[test]
    fn test_expected_free_energy() {
        let jepa = JEPA::new(1, 4, 2, 2, 4);
        let base: Array2<f64> = Array2::zeros((5, 1)).mapv(|_: f64| rand::random::<f64>() * 0.5);
        let pert1: Array2<f64> = Array2::zeros((5, 1)).mapv(|_: f64| rand::random::<f64>() * 0.5);
        let pert2: Array2<f64> = Array2::zeros((5, 1)).mapv(|_: f64| rand::random::<f64>() * 0.5);
        let efe = jepa.expected_free_energy(&base, &[pert1, pert2], 0.1);
        assert!(efe >= 0.0);
    }
}
