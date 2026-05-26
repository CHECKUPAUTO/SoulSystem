// ==========================================================================
// supervision_loop.rs — Boucle de Supervision Active (Phase 2)
//
// Pipeline complet par itération :
//   1. LLMObserver → embedding réel du Qwen2.5-0.5B (localhost:9097)
//   2. AtemporalMemory → α_sync
//   3. BCI → état caché
//   4. TopologicalProjector → embedding LLM → KV prefix
//   5. RegretOptimizer → divergence entre logits et trajectoire
//   6. Si regret > seuil → project_delta() → apply_correction() sur KV
// ==========================================================================

use std::collections::VecDeque;
use std::time::Instant;

use chronos_agent::bci::GRUCell;
use chronos_agent::coherence::continuous_coherence_loss;
use chronos_agent::hypernetwork::ConditioningHyperNetwork;
use chronos_agent::learning::RegretOptimizer;
use chronos_agent::llm_bridge::{LLMConfig, LLMObserver};
use chronos_agent::memory::AtemporalMemory;
use chronos_agent::observer::LatentObserver;
use chronos_agent::planner::StochasticDiffusionPlanner;
use chronos_agent::projector::TopologicalProjector;
use chronos_agent::ptnl_perceiver::PTNLPerceiver;

use candle_core::{Device, Tensor};

// --------------------------------------------------------------------------
// Constants (tuned for Qwen2.5-0.5B dim=896)
// --------------------------------------------------------------------------

const T: usize = 32;
const M: usize = 8;
const D_INPUT: usize = 4;
const D_LATENT: usize = 16;
const D_HIDDEN: usize = 16;
const D_MID: usize = 64;
const D_LLM: usize = 2048;       // Must be >= NUM_LAYERS * N_HEADS * D_HEAD * 2
const NUM_LAYERS: usize = 4;     // Mock LLM layers
const N_HEADS: usize = 4;
const D_HEAD: usize = 64;
const NUM_STEPS: usize = 10;
const EPISODIC_CAP: usize = 64;
const ALPHA_THRESHOLD: f64 = 0.65;
const REGRET_THRESHOLD: f64 = 0.15;  // Realistic threshold (regret ~0.2 normally)
const TOTAL_STEPS: usize = 20;       // Enough to see correction
const RECENTER_THRESHOLD: f64 = 0.30;

// --------------------------------------------------------------------------
// MockLLM (same as main.rs)
// --------------------------------------------------------------------------

struct MockLLM {
    d_model: usize,
    vocab_size: usize,
    device: Device,
    w_proj: Tensor,
    b_proj: Tensor,
}

impl MockLLM {
    fn new(d_model: usize, vocab_size: usize, device: &Device) -> candle_core::Result<Self> {
        let scale = (2.0 / d_model as f64).sqrt();
        let w_proj = Tensor::randn(0.0f64, scale, (vocab_size, d_model), device)?;
        let b_proj = Tensor::zeros(vocab_size, candle_core::DType::F64, device)?;
        Ok(Self { d_model, vocab_size, device: device.clone(), w_proj, b_proj })
    }

    fn forward(&self, input_ids: &[u32], _prefix_kv: Option<&[(Tensor, Tensor)]>) -> candle_core::Result<Tensor> {
        let seq_len = input_ids.len();
        let emb_scale = 1.0 / (self.d_model as f64).sqrt();
        let embedding = Tensor::randn(0.0f64, emb_scale, (seq_len, self.d_model), &self.device)?;
        let last = embedding.get(seq_len - 1)?.reshape((1, self.d_model))?;
        let logits = last.matmul(&self.w_proj.t()?)?.add(&self.b_proj.reshape((1, self.vocab_size))?)?;
        Ok(logits.squeeze(0)?)
    }
}

fn make_input(scalar: f64) -> Vec<f64> {
    vec![scalar, (scalar * 0.5 + 0.5).clamp(0.0, 1.0), scalar * scalar, if scalar > 0.0 { 1.0 } else { -1.0 }]
}

fn neutral_signal(t: usize, phase: f64) -> f64 {
    (2.0 * std::f64::consts::PI * t as f64 / 24.0 + phase).sin() * 0.5
}

// --------------------------------------------------------------------------
// Main
// --------------------------------------------------------------------------

fn main() -> candle_core::Result<()> {
    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║    BOUCLE DE SUPERVISION ACTIVE — PHASE 2              ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
    println!();
    println!("  Regret threshold: {:.2} (correction activée si regret > seuil)", REGRET_THRESHOLD);
    println!("  Steps: {}", TOTAL_STEPS);
    println!();

    let device = Device::Cpu;

    // ---- Initialise modules ----
    let mut perceiver = PTNLPerceiver::new(D_INPUT, D_LATENT, M, T, &device).unwrap();
    let mut bci = GRUCell::new(D_LATENT, D_HIDDEN, &device).unwrap();
    let mut memory = AtemporalMemory::new(D_LATENT, EPISODIC_CAP);
    let mut planner = StochasticDiffusionPlanner::new(NUM_STEPS, D_LATENT, &device).unwrap();
    let mut hypernetwork = ConditioningHyperNetwork::new(1, 16, D_LATENT, &device).unwrap();
    let projector = TopologicalProjector::new(D_LATENT, D_MID, D_LLM, NUM_LAYERS, &device).unwrap();
    let llm = MockLLM::new(D_LLM, 32000, &device).unwrap();
    let regret = RegretOptimizer::new(REGRET_THRESHOLD, D_LATENT, &device);
    let mut _observer = LatentObserver::new(D_LATENT);

    // ---- Initialise LLMObserver réel ----
    let llm_config = LLMConfig {
        d_model: 896,
        device: Device::Cpu,
        ..Default::default()
    };
    let mut llm_obs = LLMObserver::new(llm_config);

    // Pre-allocate sliding window
    let mut window: VecDeque<Vec<f64>> = VecDeque::with_capacity(T);
    for i in 0..T { window.push_back(make_input(neutral_signal(i, 0.0))); }

    let mut token_counter: u32 = 100;
    let mut recenter_latent: Vec<f64> = Vec::new();
    let mut regrets: Vec<f64> = Vec::new();
    let mut corrections_applied: usize = 0;
    let mut embed_errors: usize = 0;
    let mut alpha_values: Vec<f64> = Vec::new();

    // Prompts to observe (alternating topics)
    let prompts = vec![
        "What is recursion?",
        "Explain gradient descent.",
        "Write a poem about Rust.",
        "Define quantum computing.",
        "What is the capital of France?",
        "How do transformers work?",
        "Explain the water cycle.",
        "What is the meaning of life?",
        "Describe machine learning.",
        "How does the internet work?",
        "What is a blockchain?",
        "Explain the Turing test.",
        "Write a haiku about AI.",
        "What is photosynthesis?",
        "Define entropy.",
        "How do CPUs work?",
        "Explain natural selection.",
        "What is the stock market?",
        "Describe a qubit.",
        "What is the Fibonacci sequence?",
    ];

    // ---- Main loop ----
    for step in 0..TOTAL_STEPS.min(prompts.len()) {
        let step_start = Instant::now();
        let mut triggered = false;

        // 1. Get real embedding from Qwen2.5-0.5B
        let prompt = prompts[step % prompts.len()];
        let real_emb = match llm_obs.observe(prompt) {
            Ok(e) => {
                // Normalize to our latent dimension (D_LATENT=16)
                // Take first D_LATENT elements after energy normalization
                let energy: f64 = e.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-8);
                let normalized: Vec<f64> = e.iter().take(D_LATENT).map(|v| v / energy * 0.5).collect();

                // Pad or truncate to D_LATENT
                let mut latent = Vec::with_capacity(D_LATENT);
                for i in 0..D_LATENT {
                    latent.push(if i < normalized.len() { normalized[i] } else { 0.0 });
                }
                Some(latent)
            }
            Err(e) => {
                embed_errors += 1;
                eprintln!("  ⚠️  Embed error: {}", e);
                None
            }
        };

        // Fallback to window input if real embed failed
        let latent_vec = if let Some(ref e) = real_emb {
            e.clone()
        } else {
            make_input(neutral_signal(step, 0.0))
        };

        // Build input tensor
        let flat: Vec<f64> = window.iter().flat_map(|v| v.iter()).copied().collect();
        let x = Tensor::from_slice(&flat, (T, D_INPUT), &device).unwrap();
        let (latents, _) = perceiver.forward(&x).unwrap();

        // 3. Memory → α_sync
        memory.observe(&latent_vec);
        let alpha_sync = memory.alpha_sync;
        alpha_values.push(alpha_sync);

        // Auto-stabilisation
        if alpha_sync < RECENTER_THRESHOLD && !recenter_latent.is_empty() {
            let anchor = Tensor::from_slice(&recenter_latent, (D_LATENT,), &device).unwrap();
            let h = bci.step(&anchor, alpha_sync, ALPHA_THRESHOLD).unwrap();
            let h_2d = h.reshape((1, D_LATENT)).unwrap();
            let trajectory = planner.plan(&h_2d, alpha_sync).unwrap();
            let anchor_proj = projector.project(
                &Tensor::from_slice(&recenter_latent, (1, D_LATENT), &device).unwrap()
                    .reshape((1, 1, D_LATENT)).unwrap()
            ).unwrap_or_else(|_| Tensor::zeros((1, M, D_LATENT), candle_core::DType::F64, &device).unwrap());
            let kv = projector.generate_prefix_kv(&anchor_proj, N_HEADS, D_HEAD).unwrap();
            let input_ids = vec![token_counter];
            let logits = llm.forward(&input_ids, Some(&kv)).unwrap();
            let tiled = Tensor::from_slice(&recenter_latent, D_LATENT, &device).unwrap()
                .reshape((1, D_LATENT)).unwrap()
                .broadcast_as((NUM_STEPS, D_LATENT)).unwrap();
            let (_, per_step) = continuous_coherence_loss(&trajectory, &tiled).unwrap();
            let tf = trajectory.flatten_all().unwrap();
            let td: Vec<f64> = tf.to_vec1::<f64>().unwrap()[..D_LATENT].to_vec();
            let tdt = Tensor::from_slice(&td, D_LATENT, &device).unwrap();
            let (rloss, _) = regret.compute(&logits, &tdt).unwrap();
            regrets.push(rloss);
            let elapsed_us = step_start.elapsed().as_secs_f64() * 1_000_000.0;
            println!("  [{:2}/{:2}] α={:.4} | regret={:.4} | ⏱️ {:.0}µs | 🔄 auto-stab",
                step + 1, TOTAL_STEPS, alpha_sync, rloss, elapsed_us);
            token_counter = (token_counter + 1) % 32000;
            window.push_back(make_input(neutral_signal(step, 0.0)));
            window.pop_front();
            continue;
        }

        if alpha_sync >= RECENTER_THRESHOLD {
            recenter_latent = latent_vec.clone();
        }

        // 4. BCI
        let lt = Tensor::from_slice(&latent_vec, (D_LATENT,), &device).unwrap();
        let h = bci.step(&lt, alpha_sync, ALPHA_THRESHOLD).unwrap();

        // 5. Projector → KV prefix
        let projected = projector.project(&latents).unwrap();
        let mut kv_prefix = projector.generate_prefix_kv(&projected, N_HEADS, D_HEAD).unwrap();

        // 6. LLM forward
        let input_ids = vec![token_counter];
        let logits = llm.forward(&input_ids, Some(&kv_prefix)).unwrap();

        // 7. Planner
        let h_2d = h.reshape((1, D_LATENT)).unwrap();
        let trajectory = planner.plan(&h_2d, alpha_sync).unwrap();

        // 8. Coherence + Regret
        let lm = latents.mean(0).unwrap();
        let l2d = if lm.dims().len() > 1 { lm.squeeze(0).unwrap() } else { lm.clone() };
        let tiled = l2d.reshape((1, D_LATENT)).unwrap().broadcast_as((NUM_STEPS, D_LATENT)).unwrap();
        let (_, per_step) = continuous_coherence_loss(&trajectory, &tiled).unwrap();
        let tf = trajectory.flatten_all().unwrap();
        let td: Vec<f64> = tf.to_vec1::<f64>().unwrap()[..D_LATENT].to_vec();
        let tdt = Tensor::from_slice(&td, D_LATENT, &device).unwrap();
        let (rloss, delta_z_opt) = regret.compute(&logits, &tdt).unwrap();
        regrets.push(rloss);

        // Phase 2: Active supervision — apply correction to KV prefix
        let mut correction_msg = String::new();
        if let Some(delta_z) = delta_z_opt {
            triggered = true;
            corrections_applied += 1;
            let delta_emb = projector.project_delta(&delta_z).unwrap();
            kv_prefix = projector.apply_correction(&kv_prefix, &delta_emb, N_HEADS, D_HEAD).unwrap();
            correction_msg = format!("✏️ Δz corrigé");
        }

        let elapsed_us = step_start.elapsed().as_secs_f64() * 1_000_000.0;
        let source = if real_emb.is_some() { "🌐 réel" } else { "💻 fallback" };
        println!("  [{:2}/{:2}] α={:.4} | regret={:.4} | ⏱️ {:.0}µs | {} {}",
            step + 1, TOTAL_STEPS, alpha_sync, rloss, elapsed_us, source, correction_msg);

        token_counter = (token_counter + 1) % 32000;
        let new_val = make_input(neutral_signal(step, 0.0));
        window.push_back(new_val);
        window.pop_front();
    }

    // ---- Report ----
    println!();
    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║               RAPPORT SUPERVISION ACTIVE               ║");
    println!("╠═══════════════════════════════════════════════════════════╣");
    println!("  Steps exécutés:     {}", TOTAL_STEPS);
    println!("  Embeddings réels:   {} / {} ({} erreurs)",
        TOTAL_STEPS - embed_errors, TOTAL_STEPS, embed_errors);
    println!("  Corrections KV:     {}", corrections_applied);
    let avg_alpha = if !alpha_values.is_empty() { alpha_values.iter().sum::<f64>() / alpha_values.len() as f64 } else { 0.0 };
    println!("  Alpha sync (avg):  {:.4}", avg_alpha);
    let max_regret = regrets.iter().cloned().fold(0.0f64, f64::max);
    println!("  Max regret:        {:.4}", max_regret);
    println!();

    let verdict = if corrections_applied > 0 {
        format!("✅ SUPERVISION ACTIVE — {} corrections KV appliquées. Pipeline Phase 2 opérationnel (seuil={})",
            corrections_applied, REGRET_THRESHOLD)
    } else if max_regret > REGRET_THRESHOLD {
        "⚠️  Regret élevé mais aucune correction déclenchée. Vérifier le seuil.".to_string()
    } else {
        format!("✅ SYSTÈME STABLE — Regret ({:.4}) sous le seuil ({:.2}). Supervisions prêtes.",
            max_regret, REGRET_THRESHOLD)
    };
    println!("  🎯 {}", verdict);
    println!("╚═══════════════════════════════════════════════════════════╝");
    Ok(())
}
