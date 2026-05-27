// ==========================================================================
// main.rs — Chronos-Lingua: Full Orchestration Loop
//
// Pipeline per step:
//   1. Dialogue input → PTNLPerceiver (latent encoding)
//   2. TopologicalProjector (latent → LLM embedding)
//   3. LanguageModel (LLM inference with KV prefix injection)
//   4. RegretOptimizer (divergence between logits & trajectory)
//   5. HyperNetwork (Δz correction if regret > threshold)
//   6. LatentObserver (PCA monitoring + export)
// ==========================================================================

use std::collections::VecDeque;
use std::time::Instant;

use chronos_agent::bci::GRUCell;
use chronos_agent::coherence::continuous_coherence_loss;
use chronos_agent::hypernetwork::ConditioningHyperNetwork;
use chronos_agent::learning::RegretOptimizer;
use chronos_agent::memory::AtemporalMemory;
use chronos_agent::observer::LatentObserver;
use chronos_agent::planner::StochasticDiffusionPlanner;
use chronos_agent::projector::TopologicalProjector;
use chronos_agent::ptnl_perceiver::PTNLPerceiver;

use candle_core::{Device, Tensor};

// --------------------------------------------------------------------------
// Constants
// --------------------------------------------------------------------------

const T: usize = 32;                // Sliding window length
const M: usize = 8;                 // Number of latent vectors
const D_INPUT: usize = 4;           // Input dimension
const D_LATENT: usize = 16;         // Latent dimension
const D_HIDDEN: usize = 16;         // GRU hidden (= D_LATENT)
const D_MID: usize = 64;            // Projector hidden
const D_LLM: usize = 2048;  // >= NUM_LAYERS * N_HEADS * D_HEAD * 2           // LLM embedding dimension
const NUM_LAYERS: usize = 4;        // LLM layers
const N_HEADS: usize = 4;           // Attention heads per layer
const D_HEAD: usize = 64;           // Dim per head
const NUM_STEPS: usize = 10;        // Diffusion steps
const D_TIME: usize = 6;               // Time2Vec embedding dimension
const T_FUTURE: usize = 4;            // Future window for bidirectional PTNL
const EPISODIC_CAP: usize = 64;     // Episodic memory capacity
const ALPHA_THRESHOLD: f64 = 0.65;  // Insight injection threshold
const REGRET_THRESHOLD: f64 = 5.0;  // Regret trigger
const TOTAL_STEPS: usize = 64;      // Total simulation steps
const PHASE_INVERSION_AT: usize = 32; // When the phase flips
const RECENTER_THRESHOLD: f64 = 0.30; // Auto-stabilisation: if α_sync < this, recenter
// (Tuned for similarity-based α: stable ~0.5-0.9, drift <0.3)

// --------------------------------------------------------------------------
// LanguageModel trait — agnostic LLM abstraction
// --------------------------------------------------------------------------

/// Trait for any causal language model (Llama-3, Phi-3, Mistral, etc.).
pub trait LanguageModel: Send {
    /// Embedding dimension.
    fn d_model(&self) -> usize;
    /// Vocabulary size.
    fn vocab_size(&self) -> usize;
    /// Number of attention layers.
    fn num_layers(&self) -> usize;
    /// Number of heads per layer.
    fn n_heads(&self) -> usize;
    /// Head dimension.
    fn d_head(&self) -> usize;
    /// Device the model runs on.
    fn device(&self) -> &Device;

    /// Forward pass with optional KV prefix injection.
    /// Returns raw logits of shape (vocab_size,) or (1, vocab_size).
    fn forward(
        &self,
        input_ids: &[u32],
        prefix_kv: Option<&[(Tensor, Tensor)]>,
    ) -> candle_core::Result<Tensor>;
}

// --------------------------------------------------------------------------
// MockLLM — minimal placeholder for testing
// --------------------------------------------------------------------------

pub struct MockLLM {
    d_model: usize,
    vocab_size: usize,
    num_layers: usize,
    n_heads: usize,
    d_head: usize,
    device: Device,
    // Simple learned projection: embedding → logits
    w_proj: Tensor,
    b_proj: Tensor,
}

impl MockLLM {
    pub fn new(
        d_model: usize,
        vocab_size: usize,
        num_layers: usize,
        n_heads: usize,
        d_head: usize,
        device: &Device,
    ) -> candle_core::Result<Self> {
        let scale = (2.0 / d_model as f64).sqrt();
        let w_proj = Tensor::randn(0.0f64, scale, (vocab_size, d_model), device)?;
        let b_proj = Tensor::zeros(vocab_size, candle_core::DType::F64, device)?;
        Ok(Self {
            d_model, vocab_size, num_layers, n_heads, d_head,
            device: device.clone(),
            w_proj, b_proj,
        })
    }
}

impl LanguageModel for MockLLM {
    fn d_model(&self) -> usize { self.d_model }
    fn vocab_size(&self) -> usize { self.vocab_size }
    fn num_layers(&self) -> usize { self.num_layers }
    fn n_heads(&self) -> usize { self.n_heads }
    fn d_head(&self) -> usize { self.d_head }
    fn device(&self) -> &Device { &self.device }

    fn forward(
        &self,
        input_ids: &[u32],
        prefix_kv: Option<&[(Tensor, Tensor)]>,
    ) -> candle_core::Result<Tensor> {
        // Mock: generate random logits from a simple embedding + projection
        // In a real implementation, this would call the actual model forward pass.
        let seq_len = input_ids.len();

        // Simulate embedding lookup with random projection
        let emb_scale = 1.0 / (self.d_model as f64).sqrt();
        let embedding = Tensor::randn(
            0.0f64, emb_scale, (seq_len, self.d_model), &self.device,
        )?;

        // If prefix KV is provided, modify embedding based on prefix influence
        let final_emb = if let Some(kv_pairs) = prefix_kv {
            // Sum prefix KV influence into embedding
            let mut emb = embedding;
            for (k, v) in kv_pairs {
                // Average KV values and add as bias to embedding
                let kv_mean = k.mean(2)?.squeeze(1)?;  // (1, 1, d) → (1, d)
                let kv_broadcast = kv_mean.broadcast_as((seq_len, self.d_model))?;
                emb = emb.add(&kv_broadcast)?;
                let v_mean = v.mean(2)?.squeeze(1)?;
                let v_broadcast = v_mean.broadcast_as((seq_len, self.d_model))?;
                emb = emb.add(&v_broadcast)?;
            }
            emb
        } else {
            embedding
        };

        // Project to vocabulary logits: final token only
        let last_hidden = final_emb.get(seq_len - 1)?;  // (d_model,)
        let last_2d = last_hidden.reshape((1, self.d_model))?;
        let logits = last_2d.matmul(&self.w_proj.t()?)?
            .add(&self.b_proj.reshape((1, self.vocab_size))?)?;

        Ok(logits.squeeze(0)?)  // (vocab_size,)
    }
}

// --------------------------------------------------------------------------
// Data stream: dialogue-like sine with phase inversion
// --------------------------------------------------------------------------

fn generate_dialogue_stream(t: usize, phase_inversion: usize) -> Vec<f64> {
    (0..t)
        .map(|i| {
            let phase = if i >= phase_inversion {
                std::f64::consts::PI
            } else {
                0.0
            };
            // Simulated dialogue: topic shift at phase inversion
            let topic = if i >= phase_inversion {
                ((i - phase_inversion) as f64 * 0.3).sin()
            } else {
                (i as f64 * 0.1).sin()
            };
            (2.0 * std::f64::consts::PI * i as f64 / 12.0 + phase).sin() * 0.7 + topic * 0.3
        })
        .collect()
}

fn make_input(scalar: f64) -> Vec<f64> {
    vec![
        scalar,
        (scalar * 0.5 + 0.5).clamp(0.0, 1.0),
        scalar * scalar,
        if scalar > 0.0 { 1.0 } else { -1.0 },
    ]
}

// --------------------------------------------------------------------------
// Performance and monitoring stats
// --------------------------------------------------------------------------

struct ChronosStats {
    step_latencies: Vec<f64>,
    step_errors: Vec<f64>,
    step_alpha: Vec<f64>,
    step_regret: Vec<f64>,
    step_losses: Vec<f64>,
    trigger_count: usize,
}

impl ChronosStats {
    fn new(capacity: usize) -> Self {
        Self {
            step_latencies: Vec::with_capacity(capacity),
            step_errors: Vec::with_capacity(capacity),
            step_alpha: Vec::with_capacity(capacity),
            step_regret: Vec::with_capacity(capacity),
            step_losses: Vec::with_capacity(capacity),
            trigger_count: 0,
        }
    }

    fn record(&mut self, lat: f64, err: f64, alpha: f64, regret: f64, loss: f64, triggered: bool) {
        self.step_latencies.push(lat);
        self.step_errors.push(err);
        self.step_alpha.push(alpha);
        self.step_regret.push(regret);
        self.step_losses.push(loss);
        if triggered { self.trigger_count += 1; }
    }

    fn report(&self, phase_idx: usize) {
        println!();
        println!("╔═══════════════════════════════════════════════════════════╗");
        println!("║        Chronos-Lingua — Full Orchestration Report       ║");
        println!("╚═══════════════════════════════════════════════════════════╝");
        println!();

        let n = self.step_latencies.len();
        if n == 0 { return; }

        // Latency
        let avg = self.step_latencies.iter().sum::<f64>() / n as f64;
        let mx = self.step_latencies.iter().cloned().fold(0.0f64, f64::max);
        let mn = self.step_latencies.iter().cloned().fold(1e9, f64::min);
        println!("  ⏱  Latency per step:");
        println!("       Avg: {:.2} µs   Min: {:.2} µs   Max: {:.2} µs", avg, mn, mx);
        println!();

        // Error before/after phase inversion
        let pre: Vec<f64> = self.step_errors[20..phase_idx].to_vec();
        let post: Vec<f64> = self.step_errors[n.saturating_sub(20)..].to_vec();
        println!("  📉 Prediction Error (|e_t|):");
        println!("       Before topic shift:  {:.4} (avg)", pre.iter().sum::<f64>() / pre.len().max(1) as f64);
        println!("       After topic shift:   {:.4} (avg)", post.iter().sum::<f64>() / post.len().max(1) as f64);
        println!();

        // Regret
        let pre_r: Vec<f64> = self.step_regret[20..phase_idx].to_vec();
        let post_r: Vec<f64> = self.step_regret[n.saturating_sub(20)..].to_vec();
        println!("  🔄 Regret (L_regret):");
        println!("       Before: {:.4}  After: {:.4}  Triggers: {}", 
            pre_r.iter().sum::<f64>() / pre_r.len().max(1) as f64,
            post_r.iter().sum::<f64>() / post_r.len().max(1) as f64,
            self.trigger_count);
        println!();

        // Alpha sync
        let pre_a: Vec<f64> = self.step_alpha[20..phase_idx].to_vec();
        let post_a: Vec<f64> = self.step_alpha[n.saturating_sub(20)..].to_vec();
        println!("  🧠 Synchrony (α_sync):");
        println!("       Before: {:.4}  After: {:.4}",
            pre_a.iter().sum::<f64>() / pre_a.len().max(1) as f64,
            post_a.iter().sum::<f64>() / post_a.len().max(1) as f64);
        println!();

        // Coherence
        let pre_l: Vec<f64> = self.step_losses[20..phase_idx].to_vec();
        let post_l: Vec<f64> = self.step_losses[n.saturating_sub(20)..].to_vec();
        println!("  🎯 Coherence Loss (MSE):");
        println!("       Before: {:.4}  After: {:.4}",
            pre_l.iter().sum::<f64>() / pre_l.len().max(1) as f64,
            post_l.iter().sum::<f64>() / post_l.len().max(1) as f64);
        println!();

        // Last 10 steps detail
        println!("  📋 Last 10 steps:");
        println!("  {:>6}  {:>10}  {:>10}  {:>10}  {:>8}  {:>10}", "Step", "Lat(µs)", "Error", "Regret", "α_sync", "Loss");
        println!("  {:>6}  {:>10}  {:>10}  {:>10}  {:>8}  {:>10}", "------", "--------", "-------", "-------", "-------", "-------");
        let start = n.saturating_sub(10);
        for i in start..n {
            println!(
                "  {:>6}  {:>10.2}  {:>10.4}  {:>10.4}  {:>8.4}  {:>10.6}",
                i, self.step_latencies[i], self.step_errors[i],
                self.step_regret[i], self.step_alpha[i], self.step_losses[i],
            );
        }
        println!();
    }
}

// --------------------------------------------------------------------------
// Main orchestration loop
// --------------------------------------------------------------------------

/// T_axis factice pour compatibilité V6 — produit un vecteur Time2Vec neutre
/// (t=0 répété pour tous les tokens de la fenêtre).
fn make_dummy_t_axis(batch_len: usize) -> candle_core::Result<Tensor> {
    Tensor::from_slice(&vec![0.0f32; batch_len], (1, batch_len), &Device::Cpu)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║     Chronos-Lingua — Pont Holonomique + Monitoring          ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");
    println!();

    let device = Device::Cpu;
    let _rng = rand::thread_rng();

    // ---- Initialise all modules ----
    let mut perceiver = PTNLPerceiver::new(D_INPUT, D_LATENT, M, T, D_TIME, T_FUTURE, &device)?;
    let mut bci = GRUCell::new(D_LATENT, D_HIDDEN, &device)?;
    let mut memory = AtemporalMemory::new(D_LATENT, EPISODIC_CAP);
    let mut planner = StochasticDiffusionPlanner::new(NUM_STEPS, D_LATENT, &device)?;
    let mut hypernetwork = ConditioningHyperNetwork::new(1, 16, D_LATENT, &device)?;
    let projector = TopologicalProjector::new(D_LATENT, D_MID, D_LLM, NUM_LAYERS, &device)?;
    let llm = MockLLM::new(D_LLM, 32000, NUM_LAYERS, N_HEADS, D_HEAD, &device)?;
    let regret = RegretOptimizer::new(REGRET_THRESHOLD, D_LATENT, &device);
    let mut observer = LatentObserver::new(D_LATENT);

    // Pre-allocate sliding window
    let stream = generate_dialogue_stream(TOTAL_STEPS, PHASE_INVERSION_AT);
    let mut window: VecDeque<Vec<f64>> = VecDeque::with_capacity(T);
    for i in 0..T {
        window.push_back(make_input(stream[i]));
    }

    let mut stats = ChronosStats::new(TOTAL_STEPS);
    let mut token_counter: u32 = 100;  // simulated token ID
    let mut recenter_latent: Vec<f64> = Vec::new(); // Auto-stabilisation anchor

    // ---- Main loop ----
    for step in T..TOTAL_STEPS {
        let step_start = Instant::now();
        let mut triggered = false;

        // 1. Build input tensor from window
        let flat: Vec<f64> = window.iter().flat_map(|v| v.iter()).copied().collect();
        let x = Tensor::from_slice(&flat, (T, D_INPUT), &device)?;

        // 2. PTNLPerceiver: encode to latents
        let t_axis = make_dummy_t_axis(T)?;
        let (latents, _recon_loss) = perceiver.forward(&x, &t_axis)?;
        let latent_vec: Vec<f64> = {
            let lm = latents.mean(0)?;
            let l1d = if lm.dims().len() > 1 { lm.squeeze(0)? } else { lm };
            l1d.to_vec1::<f64>()?
        };
        let latent_flat: Vec<f64> = latents.flatten_all()?.to_vec1::<f64>()?;

        // 3. AtemporalMemory: observe → α_sync
        memory.observe(&latent_vec, None);
        let alpha_sync = memory.alpha_sync;

        // 3b. Auto-stabilisation: if alpha_sync collapsed, recenter on last healthy latent
        if alpha_sync < RECENTER_THRESHOLD && !latent_vec.is_empty() {
            if !recenter_latent.is_empty() {
                // Override current latent with last stable anchor
                let anchor = Tensor::from_slice(&recenter_latent, (D_LATENT,), &device)?;
                // Recompute BCI input from the anchor instead of current noisy input
                let lt = anchor;
                let h = bci.step(&lt, alpha_sync, ALPHA_THRESHOLD)?;
                // Use the anchor for planner, not the corrupted latent
                let h_2d = h.reshape((1, D_LATENT))?;
                let trajectory = planner.plan(&h_2d, alpha_sync)?;
                // Skip to projector with the anchor
                let anchor_projected = projector.project(
                    &Tensor::from_slice(&recenter_latent, (1, D_LATENT), &device)?
                        .reshape((1, 1, D_LATENT))?
                ).unwrap_or_else(|_| {
                    // Fallback: create a zero projection
                    Tensor::zeros((1, M, D_LATENT), candle_core::DType::F64, &device).unwrap()
                });
                let kv_prefix = projector.generate_prefix_kv(
                    &anchor_projected, N_HEADS, D_HEAD,
                )?;
                let input_ids = vec![token_counter];
                let logits = llm.forward(&input_ids, Some(&kv_prefix))?;
                // Coherence from anchor
                let latent_tiled = Tensor::from_slice(&recenter_latent, D_LATENT, &device)?
                    .reshape((1, D_LATENT))?
                    .broadcast_as((NUM_STEPS, D_LATENT))?;
                let (coherence_loss, per_step_err) =
                    continuous_coherence_loss(&trajectory, &latent_tiled)?;
                let e_t = per_step_err[0].abs();
                let traj_flat = trajectory.flatten_all()?;
                let traj_first_d: Vec<f64> = traj_flat.to_vec1::<f64>()?[..D_LATENT].to_vec();
                let traj_d = Tensor::from_slice(&traj_first_d, D_LATENT, &device)?;
                let (regret_loss, delta_z_opt) = regret.compute(&logits, &traj_d)?;
                let combined_error = e_t + regret_loss * 0.1;
                let dz = hypernetwork.forward(combined_error)?;
                triggered = regret_loss > REGRET_THRESHOLD;
                let elapsed_us = step_start.elapsed().as_secs_f64() * 1_000_000.0;
                stats.record(elapsed_us, e_t, alpha_sync, regret_loss, coherence_loss, triggered);
                token_counter = (token_counter + 1) % 32000;
                let new_val = make_input(stream[step]);
                window.push_back(new_val);
                window.pop_front();
                continue;
            }
        }

        // Store current latent as potential future anchor (only if healthy)
        if alpha_sync >= RECENTER_THRESHOLD {
            recenter_latent = latent_vec.clone();
        }

        // 4. BCI step
        let lt = Tensor::from_slice(&latent_vec, (D_LATENT,), &device)?;
        let h = bci.step(&lt, alpha_sync, ALPHA_THRESHOLD)?;

        // 5. TopologicalProjector: latent → LLM embedding → KV prefix
        let projected = projector.project(&latents)?;
        let kv_prefix = projector.generate_prefix_kv(
            &projected, N_HEADS, D_HEAD,
        )?;

        // 6. LanguageModel: forward with KV injection
        let input_ids = vec![token_counter];
        let logits = llm.forward(&input_ids, Some(&kv_prefix))?;

        // 7. Planner: generate ideal trajectory
        let h_2d = h.reshape((1, D_LATENT))?;
        let trajectory = planner.plan(&h_2d, alpha_sync)?;

        // 8. Coherence loss
        let latent_mean = latents.mean(0)?;
        let latent_2d = if latent_mean.dims().len() > 1 {
            latent_mean.squeeze(0)?
        } else {
            latent_mean.clone()
        };
        let latent_tiled = latent_2d.reshape((1, D_LATENT))?
            .broadcast_as((NUM_STEPS, D_LATENT))?;
        let (coherence_loss, per_step_err) =
            continuous_coherence_loss(&trajectory, &latent_tiled)?;
        let e_t = per_step_err[0].abs();

        // 9. RegretOptimizer: divergence between LLM logits & trajectory
        let traj_flat = trajectory.flatten_all()?;
        let traj_first_d: Vec<f64> = traj_flat.to_vec1::<f64>()?[..D_LATENT].to_vec();
        let traj_d = Tensor::from_slice(&traj_first_d, D_LATENT, &device)?;
        let (regret_loss, delta_z_opt) = regret.compute(&logits, &traj_d)?;

        // Phase 2: Hidden-state supervision
        // If regret exceeds threshold, project delta_z through the projector
        // and apply it directly to the KV prefix for the NEXT step.
        let kv_prefix_used = if let Some(delta_z) = delta_z_opt {
            triggered = true;
            let delta_emb = projector.project_delta(&delta_z)?;
            projector.apply_correction(&kv_prefix, &delta_emb, N_HEADS, D_HEAD)?
        } else {
            kv_prefix.clone()
        };
        let _ = kv_prefix_used; // Will be used on next iteration when KV is cached

        // 10. LatentObserver: PCA on latent buffer
        observer.observe(&latent_flat, M, D_LATENT);

        // Advance sliding window
        token_counter = (token_counter + 1) % 32000;
        let new_val = make_input(stream[step]);
        window.push_back(new_val);
        window.pop_front();

        // Record stats
        let elapsed_us = step_start.elapsed().as_secs_f64() * 1_000_000.0;
        stats.record(elapsed_us, e_t, alpha_sync, regret_loss, coherence_loss, triggered);
    }

    // ---- Report ----
    stats.report(PHASE_INVERSION_AT);

    // Export observer data
    let csv_data = observer.export_csv();
    println!("  📤 Observer CSV ({} bytes exported)", csv_data.len());
    println!("  📤 Observer JSON ({} bytes exported)", observer.export_json().len());
    println!();
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║          Chronos-Lingua simulation complete.                ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");

    Ok(())
}
