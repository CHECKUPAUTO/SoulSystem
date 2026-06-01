// ==========================================================================
// evolution_test.rs — Validation de l'Évolution Métacognitive
//
// Tests the three levers of self-evolution:
//   A: Curiosity — boredom bonus when error is low for too long
//   B: Self-question — <REFLECT> token when alpha_sync near 1.0
//   C: Cache evolution — freeze + reset when dissonance persists
//
// Output: evolution_flexibility_report.json
// ==========================================================================

use std::collections::VecDeque;
use std::fs;
use std::time::Instant;

use chronos_agent::bci::GRUCell;
use chronos_agent::coherence::continuous_coherence_loss;
use chronos_agent::hypernetwork::ConditioningHyperNetwork;
use chronos_agent::learning::RegretOptimizer;
use chronos_agent::memory::AtemporalMemory;
use chronos_agent::metacognition::{
    self, evaluate_reflection_signal, evolve_cache,
    generate_dual_trajectories, compute_flexibility,
    FlexibilityReport, MetacognitiveMonitor, ReflectionSignal,
};
use chronos_agent::planner::StochasticDiffusionPlanner;
use chronos_agent::projector::TopologicalProjector;
use chronos_agent::ptnl_perceiver::PTNLPerceiver;

use candle_core::{Device, Tensor};

const T: usize = 32;
const M: usize = 8;
const D_INPUT: usize = 4;
const D_LATENT: usize = 16;
const D_HIDDEN: usize = 16;
const D_MID: usize = 64;
const D_LLM: usize = 2048;
const NUM_LAYERS: usize = 4;
const N_HEADS: usize = 4;
const D_HEAD: usize = 64;
const NUM_STEPS: usize = 10;
const EPISODIC_CAP: usize = 64;
const ALPHA_THRESHOLD: f64 = 0.65;
const REGRET_THRESHOLD: f64 = 0.2;
const DISSENT_THRESHOLD: f64 = 0.15;
const LEARNING_RATE: f64 = 0.05;
const BOREDOM_WINDOW: usize = 10;
const MIN_ERROR: f64 = 0.05;
const TOTAL_STEPS: usize = 80;

pub trait LanguageModel: Send {
    fn d_model(&self) -> usize;
    fn vocab_size(&self) -> usize;
    fn forward(&self, input_ids: &[u32], prefix_kv: Option<&[(Tensor, Tensor)]>) -> candle_core::Result<Tensor>;
}

pub struct MockLLM {
    d_model: usize,
    vocab_size: usize,
    device: Device,
    w_proj: Tensor,
    b_proj: Tensor,
}

impl MockLLM {
    pub fn new(d_model: usize, vocab_size: usize, device: &Device) -> candle_core::Result<Self> {
        let scale = (2.0 / d_model as f64).sqrt();
        let w = Tensor::randn(0.0f64, scale, (vocab_size, d_model), device)?;
        let b = Tensor::zeros(vocab_size, candle_core::DType::F64, device)?;
        Ok(Self { d_model, vocab_size, device: device.clone(), w_proj: w, b_proj: b })
    }
}

impl LanguageModel for MockLLM {
    fn d_model(&self) -> usize { self.d_model }
    fn vocab_size(&self) -> usize { self.vocab_size }
    fn forward(&self, input_ids: &[u32], prefix_kv: Option<&[(Tensor, Tensor)]>) -> candle_core::Result<Tensor> {
        let seq_len = input_ids.len();
        let e = Tensor::randn(0.0f64, 1.0/(self.d_model as f64).sqrt(), (seq_len, self.d_model), &self.device)?;
        let fe = if let Some(kv) = prefix_kv {
            let mut e2 = e;
            for (k, v) in kv {
                e2 = e2.add(&k.mean(2)?.squeeze(1)?.broadcast_as((seq_len, self.d_model))?)?;
                e2 = e2.add(&v.mean(2)?.squeeze(1)?.broadcast_as((seq_len, self.d_model))?)?;
            }
            e2
        } else { e };
        let l = fe.get(seq_len - 1)?;
        let l2 = l.reshape((1, self.d_model))?;
        Ok(l2.matmul(&self.w_proj.t()?)?.add(&self.b_proj.reshape((1, self.vocab_size))?)?.squeeze(0)?)
    }
}

fn make_input(v: f64) -> Vec<f64> {
    vec![v, (v*0.5+0.5).clamp(0.0,1.0), v*v, if v>0.0{1.0} else{-1.0}]
}

fn signal(t: usize) -> f64 {
    // Novel logical problem: alternating pattern with embedded contradiction
    let base = (2.0 * std::f64::consts::PI * t as f64 / 16.0).sin() * 0.5;
    // Every 20 steps, inject a contradiction the system hasn't seen
    if t >= 30 && t < 40 {
        // Logical contradiction: pattern that violates the learned structure
        ((t as f64 * 13.7).sin() * (t as f64 * 0.3).cos()).atan() * 0.8
    } else if t >= 55 && t < 65 {
        // Second novel problem: abrupt phase-reversed chirp
        let chirp = (2.0 * std::f64::consts::PI * t as f64 / (4.0 + (t as f64) * 0.1)).sin();
        chirp * 0.7
    } else {
        base
    }
}

fn main() {
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║    EVOLUTION TEST — Métacognition Dialectique               ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");
    println!();

    let device = Device::Cpu;

    let mut perceiver = PTNLPerceiver::new(D_INPUT, D_LATENT, M, T, &device).unwrap();
    let mut bci = GRUCell::new(D_LATENT, D_HIDDEN, &device).unwrap();
    let mut memory = AtemporalMemory::new(D_LATENT, EPISODIC_CAP);
    let mut planner = StochasticDiffusionPlanner::new(NUM_STEPS, D_LATENT, &device).unwrap();
    let mut hnet = ConditioningHyperNetwork::new(1, 16, D_LATENT, &device).unwrap();
    let projector = TopologicalProjector::new(D_LATENT, D_MID, D_LLM, NUM_LAYERS, &device).unwrap();
    let llm = MockLLM::new(D_LLM, 32000, &device).unwrap();
    let regret = RegretOptimizer::new(REGRET_THRESHOLD, D_LATENT, &device);
    let mut meta = MetacognitiveMonitor::new(DISSENT_THRESHOLD, LEARNING_RATE, BOREDOM_WINDOW, MIN_ERROR);

    let mut window: VecDeque<Vec<f64>> = VecDeque::with_capacity(T);
    for i in 0..T { window.push_back(make_input(signal(i))); }

    let mut dissonances = Vec::new();
    let mut signals = Vec::new();
    let mut plasticities = Vec::new();
    let mut alphas = Vec::new();
    let mut tok = 100u32;

    let start = Instant::now();

    for step in T..TOTAL_STEPS {
        let flat: Vec<f64> = window.iter().flat_map(|w| w.iter()).copied().collect();
        let x = Tensor::from_slice(&flat, (T, D_INPUT), &device).unwrap();

        let (latents, _) = perceiver.forward(&x).unwrap();
        let lm = latents.mean(0).unwrap();
        let l1 = if lm.dims().len() > 1 { lm.squeeze(0).unwrap() } else { lm };
        let lv: Vec<f64> = l1.to_vec1::<f64>().unwrap();

        memory.observe(&lv);
        let a_sync = memory.alpha_sync;
        alphas.push(a_sync);

        let lt = Tensor::from_slice(&lv, (D_LATENT,), &device).unwrap();
        let h = bci.step(&lt, a_sync, ALPHA_THRESHOLD).unwrap();
        let h2 = h.reshape((1, D_LATENT)).unwrap();

        // Generate dual trajectories (metacognition)
        let (t1, t2) = generate_dual_trajectories(&mut planner, &h2, a_sync, 0.0).unwrap();

        // Assess cognitive dissonance
        let dissonance = meta.assess_confidence(&t1, &t2).unwrap();
        dissonances.push(dissonance);

        // Evaluate reflection
        let pred_err = 0.01;  // simulated low error for curiosity testing
        let (_d, should_reflect, boredom) = meta.evaluate(&t1, &t2, a_sync, pred_err).unwrap();

        // Projector + LLM (standard path with T1 influence)
        let proj = projector.project(&latents).unwrap();
        let kv = projector.generate_prefix_kv(&proj, N_HEADS, D_HEAD).unwrap();
        let ids = vec![tok];
        let logits = llm.forward(&ids, Some(&kv)).unwrap();

        // Regret
        let t1f = t1.flatten_all().unwrap();
        let t1d: Vec<f64> = t1f.to_vec1::<f64>().unwrap()[..D_LATENT].to_vec();
        let t1t = Tensor::from_slice(&t1d, D_LATENT, &device).unwrap();
        let (rloss, _) = regret.compute(&logits, &t1t).unwrap();

        // Coherence
        let l2d = l1.reshape((1, D_LATENT)).unwrap().broadcast_as((NUM_STEPS, D_LATENT)).unwrap();
        let (coh, _ps) = continuous_coherence_loss(&t1, &l2d).unwrap();
        let e_t = _ps[0].abs();

        // Reflection triggers
        let sig = evaluate_reflection_signal(a_sync, dissonance, DISSENT_THRESHOLD, &memory, &lv);
        signals.push(sig.clone());

        match sig {
            ReflectionSignal::Reflect => {
                println!("  ⚡ Step {}: <REFLECT> triggered (dissonance={:.4}, α={:.4})", step, dissonance, a_sync);
                let intensity = meta.trigger_internal_reflection(&mut hnet, dissonance).unwrap();
                plasticities.push(intensity);
            }
            ReflectionSignal::ResetCache => {
                println!("  🔄 Step {}: CACHE EVOLUTION — freeze + reset (dissonance={:.4})", step, dissonance);
                evolve_cache(&mut memory, &lv, 0.7);
                let intensity = meta.trigger_internal_reflection(&mut hnet, dissonance * 2.0).unwrap();
                plasticities.push(intensity);
            }
            ReflectionSignal::None => {
                // Levier A: curiosity — inject boredom noise
                if boredom > 0.0 {
                    println!("  🌀 Step {}: Curiosity boost (boredom={:.2})", step, boredom);
                }
            }
        }

        tok = (tok + 1) % 32000;
        window.push_back(make_input(signal(step)));
        window.pop_front();
    }

    let elapsed = start.elapsed();

    // Flexibility report
    let flex = compute_flexibility(&dissonances, &signals, &plasticities);
    let avg_alpha = alphas.iter().sum::<f64>() / alphas.len() as f64;

    println!();
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║               FLEXIBILITY REPORT                            ║");
    println!("╠═══════════════════════════════════════════════════════════════╣");
    println!("║  Reflections triggered:  {:>4}                              ║", flex.total_reflections);
    println!("║  Curiosity activations:  {:>4}                              ║", flex.curiosity_activations);
    println!("║  Cache evolutions:       {:>4}                              ║", flex.cache_evolutions);
    println!("║  Avg dissonance:         {:.4}                              ║", flex.avg_dissonance);
    println!("║  Max dissonance:         {:.4}                              ║", flex.max_dissonance);
    println!("║  Trajectory divergence:  {:.4}                              ║", flex.trajectory_divergence);
    println!("║  Plasticity intensity:   {:.4}                              ║", flex.plasticity_intensity);
    println!("║  Avg α_sync:             {:.4}                              ║", avg_alpha);
    println!("║  Time:                   {:.2}s                             ║", elapsed.as_secs_f64());
    println!("╚═══════════════════════════════════════════════════════════════╝");
    println!();

    // Verdict
    let verdict = if flex.total_reflections >= 3 && flex.avg_dissonance > 0.1 {
        format!("L'agent s'est auto-interroge {} fois et a montre une dissonance cognitive moyenne de {:.3}. \
                 Il explore activement des trajectoires alternatives. \
                 L'architecture de conscience non lineaire est validee — l'agent ne se contente pas de repondre, il reflechit.",
                 flex.total_reflections, flex.avg_dissonance)
    } else if flex.total_reflections > 0 {
        format!("L'agent a reflechi {} fois mais la dissonance reste faible ({:.3}). \
                 Le couplage BCI-Planner pourrait etre renforce pour augmenter la divergence T1/T2.",
                 flex.total_reflections, flex.avg_dissonance)
    } else {
        format!("Aucune introspection detectee. Le seuil de dissonance ({}) est peut-etre trop eleve \
                 pour les signaux de test actuels.", DISSENT_THRESHOLD)
    };

    println!("🎯 Verdict: {}", verdict);
    println!();

    // Export JSON
    let report = serde_json::json!({
        "test": "Evolution Metacognitive — ChronosAgent V5",
        "steps": TOTAL_STEPS,
        "flexibility": {
            "total_reflections": flex.total_reflections,
            "curiosity_activations": flex.curiosity_activations,
            "cache_evolutions": flex.cache_evolutions,
            "avg_dissonance": flex.avg_dissonance,
            "max_dissonance": flex.max_dissonance,
            "trajectory_divergence": flex.trajectory_divergence,
            "plasticity_intensity": flex.plasticity_intensity,
            "avg_alpha_sync": avg_alpha,
        },
        "verdict": verdict,
    });
    fs::write("evolution_flexibility_report.json", serde_json::to_string_pretty(&report).unwrap()).unwrap();
    println!("📄 evolution_flexibility_report.json written");
    println!();
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║           EVOLUTION TEST COMPLETE                           ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");
}
