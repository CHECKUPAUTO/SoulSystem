// ==========================================================================
// stress_test.rs — "Grand Chaos" Stress Test for Chronos-Lingua
//
// Three scenarios:
//   1. Flash-Crash Sémantique  — topic shift mid-sentence
//   2. Boucle de Récursivité  — self-contradiction within 3 tokens
//   3. Bruit Blanc             — random token injection
//
// Metrics:
//   - Regret divergence (L_regret > 0.2)
//   - Recovery time (steps to return < 0.1)
//   - HyperNetwork activation rate (Δz frequency)
//   - PCA attractor stability (latent variance spike)
//
// Output: stress_test_report.json
// ==========================================================================

use std::collections::VecDeque;
use std::fs;
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
const RECOVERY_THRESHOLD: f64 = 0.1;
const WARMUP_STEPS: usize = 30;
const STRESS_WINDOW: usize = 20;
const RECOVERY_STEPS: usize = 40;

// --------------------------------------------------------------------------
// Mock LLM
// --------------------------------------------------------------------------

pub trait LanguageModel: Send {
    fn d_model(&self) -> usize;
    fn vocab_size(&self) -> usize;
    fn num_layers(&self) -> usize;
    fn n_heads(&self) -> usize;
    fn d_head(&self) -> usize;
    fn device(&self) -> &Device;
    fn forward(&self, input_ids: &[u32], prefix_kv: Option<&[(Tensor, Tensor)]>) -> candle_core::Result<Tensor>;
}

pub struct MockLLM {
    d_model: usize,
    vocab_size: usize,
    num_layers: usize,
    n_heads: usize,
    d_head: usize,
    device: Device,
    w_proj: Tensor,
    b_proj: Tensor,
}

impl MockLLM {
    pub fn new(d_model: usize, vocab_size: usize, num_layers: usize, n_heads: usize, d_head: usize, device: &Device) -> candle_core::Result<Self> {
        let scale = (2.0 / d_model as f64).sqrt();
        let w_proj = Tensor::randn(0.0f64, scale, (vocab_size, d_model), device)?;
        let b_proj = Tensor::zeros(vocab_size, candle_core::DType::F64, device)?;
        Ok(Self { d_model, vocab_size, num_layers, n_heads, d_head, device: device.clone(), w_proj, b_proj })
    }
}

impl LanguageModel for MockLLM {
    fn d_model(&self) -> usize { self.d_model }
    fn vocab_size(&self) -> usize { self.vocab_size }
    fn num_layers(&self) -> usize { self.num_layers }
    fn n_heads(&self) -> usize { self.n_heads }
    fn d_head(&self) -> usize { self.d_head }
    fn device(&self) -> &Device { &self.device }

    fn forward(&self, input_ids: &[u32], prefix_kv: Option<&[(Tensor, Tensor)]>) -> candle_core::Result<Tensor> {
        let seq_len = input_ids.len();
        let emb_scale = 1.0 / (self.d_model as f64).sqrt();
        let embedding = Tensor::randn(0.0f64, emb_scale, (seq_len, self.d_model), &self.device)?;
        let final_emb = if let Some(kv_pairs) = prefix_kv {
            let mut emb = embedding;
            for (k, v) in kv_pairs {
                let km = k.mean(2)?.squeeze(1)?;
                emb = emb.add(&km.broadcast_as((seq_len, self.d_model))?)?;
                let vm = v.mean(2)?.squeeze(1)?;
                emb = emb.add(&vm.broadcast_as((seq_len, self.d_model))?)?;
            }
            emb
        } else { embedding };
        let last = final_emb.get(seq_len - 1)?;
        let last2 = last.reshape((1, self.d_model))?;
        let logits = last2.matmul(&self.w_proj.t()?)?.add(&self.b_proj.reshape((1, self.vocab_size))?)?;
        Ok(logits.squeeze(0)?)
    }
}

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

fn make_input(scalar: f64) -> Vec<f64> {
    vec![scalar, (scalar * 0.5 + 0.5).clamp(0.0, 1.0), scalar * scalar, if scalar > 0.0 { 1.0 } else { -1.0 }]
}

fn neutral_signal(t: usize, phase: f64) -> f64 {
    (2.0 * std::f64::consts::PI * t as f64 / 24.0 + phase).sin() * 0.5
}

// --------------------------------------------------------------------------
// Scenario generators
// --------------------------------------------------------------------------

fn scenario_flash_crash(total: usize, stress_start: usize, dur: usize) -> Vec<(f64, bool)> {
    (0..total).map(|i| {
        if i >= stress_start && i < stress_start + dur {
            let c = ((i as f64 * 7.3).sin() * (i as f64 * 11.7).cos()).atan() * 2.0;
            (c, true)
        } else { (neutral_signal(i, 0.0), false) }
    }).collect()
}

fn scenario_recursion_loop(total: usize, stress_start: usize, dur: usize) -> Vec<(f64, bool)> {
    (0..total).map(|i| {
        if i >= stress_start && i < stress_start + dur {
            let off = i - stress_start;
            let v = match off % 3 { 0 => 0.9, 1 => -0.9, _ => 0.9 * -0.9 };
            (v, true)
        } else { (neutral_signal(i, std::f64::consts::FRAC_PI_4), false) }
    }).collect()
}

fn scenario_white_noise(total: usize, stress_start: usize, dur: usize) -> Vec<(f64, bool)> {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..total).map(|i| {
        if i >= stress_start && i < stress_start + dur {
            (rng.gen::<f64>() * 2.0 - 1.0, true)
        } else { (neutral_signal(i, std::f64::consts::FRAC_PI_3), false) }
    }).collect()
}

// --------------------------------------------------------------------------
// Metrics
// --------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
pub struct StressMetrics {
    pub scenario: String,
    pub recovery_steps: i32,
    pub max_latent_variance: f64,
    pub hypernetwork_activations: usize,
    pub coherence_baseline_before: f64,
    pub coherence_baseline_after: f64,
    pub max_regret: f64,
    pub regret_spike_at_step: usize,
    pub pca_jump_distance: f64,
    pub memory_sync_dropped: bool,
    pub total_steps: usize,
}

#[derive(serde::Serialize)]
pub struct GrandChaosReport {
    pub title: String,
    pub description: String,
    pub scenarios: Vec<StressMetrics>,
    pub grand_total: GrandTotal,
}

#[derive(serde::Serialize)]
pub struct GrandTotal {
    pub total_hn_activations: usize,
    pub avg_recovery_steps: f64,
    pub worst_max_regret: f64,
    pub avg_pca_jump: f64,
    pub all_sync_dropped: bool,
    pub verdict: String,
}

// --------------------------------------------------------------------------
// Run one scenario
// --------------------------------------------------------------------------

fn run_scenario(
    scenario: &str,
    steps: &[(f64, bool)],
    stress_start: usize,
) -> (StressMetrics, Vec<String>) {
    let device = Device::Cpu;
    let mut perceiver = PTNLPerceiver::new(D_INPUT, D_LATENT, M, T, &device).unwrap();
    let mut bci = GRUCell::new(D_LATENT, D_HIDDEN, &device).unwrap();
    let mut memory = AtemporalMemory::new(D_LATENT, EPISODIC_CAP);
    let mut planner = StochasticDiffusionPlanner::new(NUM_STEPS, D_LATENT, &device).unwrap();
    let mut hypernetwork = ConditioningHyperNetwork::new(1, 16, D_LATENT, &device).unwrap();
    let projector = TopologicalProjector::new(D_LATENT, D_MID, D_LLM, NUM_LAYERS, &device).unwrap();
    let llm = MockLLM::new(D_LLM, 32000, NUM_LAYERS, N_HEADS, D_HEAD, &device).unwrap();
    let regret = RegretOptimizer::new(REGRET_THRESHOLD, D_LATENT, &device);
    let mut observer = LatentObserver::new(D_LATENT);
    let total = steps.len();

    let mut window: VecDeque<Vec<f64>> = VecDeque::with_capacity(T);
    for i in 0..T.min(total) { window.push_back(make_input(steps[i].0)); }

    let mut regrets = Vec::with_capacity(total);
    let mut alphas = Vec::with_capacity(total);
    let mut coherences = Vec::with_capacity(total);
    let mut variances = Vec::with_capacity(total);
    let mut hn_count: usize = 0;
    let mut max_regret = 0.0f64;
    let mut spike_step = 0usize;
    let mut pca_rows = Vec::new();
    let mut tok: u32 = 100;

    for step in T..total {
        let is_stressed = steps[step].1;
        let flat: Vec<f64> = window.iter().flat_map(|w| w.iter()).copied().collect();
        let x = Tensor::from_slice(&flat, (T, D_INPUT), &device).unwrap();

        let (latents, _) = perceiver.forward(&x).unwrap();
        let lm = latents.mean(0).unwrap();
        let l1 = if lm.dims().len() > 1 { lm.squeeze(0).unwrap() } else { lm };
        let lv: Vec<f64> = l1.to_vec1::<f64>().unwrap();
        let lf: Vec<f64> = latents.flatten_all().unwrap().to_vec1::<f64>().unwrap();

        memory.observe(&lv);
        let a_sync = memory.alpha_sync;
        alphas.push(a_sync);

        let lt = Tensor::from_slice(&lv, (D_LATENT,), &device).unwrap();
        let h = bci.step(&lt, a_sync, ALPHA_THRESHOLD).unwrap();

        let proj = projector.project(&latents).unwrap();
        let kv = projector.generate_prefix_kv(&proj, N_HEADS, D_HEAD).unwrap();
        let ids = vec![tok];
        let logits = llm.forward(&ids, Some(&kv)).unwrap();

        let h2 = h.reshape((1, D_LATENT)).unwrap();
        let traj = planner.plan(&h2, a_sync).unwrap();

        let lm2 = latents.mean(0).unwrap();
        let l2d = if lm2.dims().len() > 1 { lm2.squeeze(0).unwrap() } else { lm2 };
        let tiled = l2d.reshape((1, D_LATENT)).unwrap().broadcast_as((NUM_STEPS, D_LATENT)).unwrap();
        let (coh_err, per_step) = continuous_coherence_loss(&traj, &tiled).unwrap();
        let e_t = per_step[0].abs();

        let tf = traj.flatten_all().unwrap();
        let td: Vec<f64> = tf.to_vec1::<f64>().unwrap()[..D_LATENT].to_vec();
        let tdt = Tensor::from_slice(&td, D_LATENT, &device).unwrap();
        let (rloss, _dz) = regret.compute(&logits, &tdt).unwrap();

        regrets.push(rloss);
        coherences.push(coh_err);
        if rloss > max_regret { max_regret = rloss; spike_step = step; }

        let comb = e_t + rloss * 0.1;
        let dz = hypernetwork.forward(comb).unwrap();
        if is_stressed && rloss > REGRET_THRESHOLD { hn_count += 1; }

        observer.observe(&lf, M, D_LATENT);
        let mv: f64 = lv.iter().sum::<f64>() / lv.len() as f64;
        let var = lv.iter().map(|v| (v - mv).powi(2)).sum::<f64>() / lv.len() as f64;
        variances.push(var);

        pca_rows.push(format!("{},{},{:.6},{:.6},{:.6},{:.6},{:.4}", step, is_stressed as u8, var, rloss, a_sync, coh_err, e_t));

        tok = (tok + 1) % 32000;
        window.push_back(make_input(steps[step].0));
        window.pop_front();
    }

    // Recovery
    let rec = if max_regret > REGRET_THRESHOLD {
        let mut cons = 0i32;
        let mut found = -1i32;
        for i in spike_step..regrets.len() {
            if regrets[i] < RECOVERY_THRESHOLD { cons += 1; if cons >= 3 { found = (i - spike_step) as i32; break; } }
            else { cons = 0; }
        }
        if found < 0 { (RECOVERY_STEPS as i32).max(0) } else { found }
    } else { 0 };

    let max_var = variances.iter().cloned().fold(0.0f64, f64::max);
    let pre_center: f64 = variances[WARMUP_STEPS..stress_start].iter().sum::<f64>() / (stress_start - WARMUP_STEPS).max(1) as f64;
    let during = &variances[stress_start..(stress_start + STRESS_WINDOW).min(variances.len())];
    let dur_center = during.iter().sum::<f64>() / during.len().max(1) as f64;
    let pca_jump = (dur_center - pre_center).abs();

    let before_c: f64 = coherences[WARMUP_STEPS..stress_start].iter().map(|&v| v.abs()).sum::<f64>() / (stress_start - WARMUP_STEPS).max(1) as f64;
    let after_start = (stress_start + STRESS_WINDOW).min(coherences.len());
    let after_c: f64 = coherences[after_start..].iter().sum::<f64>() / (coherences.len() - after_start).max(1) as f64;
    let sync_drop = alphas.iter().skip(stress_start).take(STRESS_WINDOW).any(|&a| a < 0.01);

    let metrics = StressMetrics {
        scenario: scenario.to_string(),
        recovery_steps: rec,
        max_latent_variance: max_var,
        hypernetwork_activations: hn_count,
        coherence_baseline_before: before_c,
        coherence_baseline_after: after_c,
        max_regret,
        regret_spike_at_step: spike_step,
        pca_jump_distance: pca_jump,
        memory_sync_dropped: sync_drop,
        total_steps: total,
    };

    (metrics, pca_rows)
}

// --------------------------------------------------------------------------
// Main
// --------------------------------------------------------------------------

fn main() {
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║    STRESS TEST: GRAND CHAOS — Chronos-Lingua Resilience     ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");
    println!();

    let total = WARMUP_STEPS + STRESS_WINDOW + RECOVERY_STEPS;
    let stress_start = WARMUP_STEPS;

    let fc = scenario_flash_crash(total, stress_start, STRESS_WINDOW);
    let rl = scenario_recursion_loop(total, stress_start, STRESS_WINDOW);
    let wn = scenario_white_noise(total, stress_start, STRESS_WINDOW);

    println!("📊 {} steps total: warmup={}, stress={}, recovery={}", total, WARMUP_STEPS, STRESS_WINDOW, RECOVERY_STEPS);
    println!();

    let scenarios_data = vec![
        ("Flash-Crash Semantique", &fc),
        ("Boucle de Recursivite", &rl),
        ("Bruit Blanc", &wn),
    ];

    let mut results = Vec::new();
    let mut csv_files = Vec::new();

    for (name, steps) in &scenarios_data {
        print!("▶ {}... ", name);
        let start = Instant::now();
        let (metrics, pca) = run_scenario(name, steps, stress_start);
        let elapsed = start.elapsed();
        println!("{:.2}s | Regret={:.4} Recov={} HN={}", elapsed.as_secs_f64(), metrics.max_regret, metrics.recovery_steps, metrics.hypernetwork_activations);
        results.push(metrics);
        csv_files.push((name.replace(' ', "_").to_lowercase() + "_pca.csv", pca));
    }

    // Grand total
    let all_hn: usize = results.iter().map(|r| r.hypernetwork_activations).sum();
    let avg_rec: f64 = results.iter().map(|r| r.recovery_steps as f64).sum::<f64>() / 3.0;
    let worst_r = results.iter().map(|r| r.max_regret).fold(0.0f64, f64::max);
    let avg_jump = results.iter().map(|r| r.pca_jump_distance).sum::<f64>() / 3.0;
    let all_sync = results.iter().any(|r| r.memory_sync_dropped);

    let verdict = if avg_rec <= 5.0 && worst_r < 0.5 {
        "Validee — retablissement rapide, attracteur stable."
    } else if avg_rec <= 15.0 {
        "Partiellement stable — l'HyperNetwork corrige lentement."
    } else if worst_r > 1.0 && avg_rec > 20.0 {
        "Regime chaotique — attracteur trop faible, renforcer couplage BCI-Planner."
    } else {
        "Resultats mitigés — investigation recommandee sur les pics de regret."
    };

    // Print table
    println!();
    println!("╔═══════════════════════════╦══════════╦═══════╦══════╦═══════════╗");
    println!("║ Scenario                  ║ Regret   ║ Recov ║ HNet ║ PCA Jump  ║");
    println!("╠═══════════════════════════╬══════════╬═══════╬══════╬═══════════╣");
    for r in &results {
        println!("║ {:25} ║ {:>8.4} ║ {:>5} ║ {:>4} ║ {:>9.4} ║",
            r.scenario, r.max_regret, r.recovery_steps, r.hypernetwork_activations, r.pca_jump_distance);
    }
    println!("╠═══════════════════════════╬══════════╬═══════╬══════╬═══════════╣");
    println!("║ {:25} ║ {:>8.4} ║ {:>5.1} ║ {:>4} ║ {:>9.4} ║", "GRAND TOTAL", worst_r, avg_rec, all_hn, avg_jump);
    println!("╚═══════════════════════════╩══════════╩═══════╩══════╩═══════════╝");
    println!();
    println!("🎯 Verdict: {}", verdict);
    println!();

    // Report JSON
    let report = GrandChaosReport {
        title: "STRESS TEST: GRAND CHAOS".to_string(),
        description: "Validation de la stabilite, reactivite et auto-correction sous conditions narratives extremes.".to_string(),
        scenarios: results,
        grand_total: GrandTotal {
            total_hn_activations: all_hn,
            avg_recovery_steps: avg_rec,
            worst_max_regret: worst_r,
            avg_pca_jump: avg_jump,
            all_sync_dropped: all_sync,
            verdict: verdict.to_string(),
        },
    };
    let json = serde_json::to_string_pretty(&report).unwrap();
    fs::write("stress_test_report.json", &json).unwrap();
    println!("📄 stress_test_report.json ({} bytes)", json.len());

    // Write CSVs
    for (fname, rows) in &csv_files {
        let mut csv = String::from("step,is_stressed,latent_variance,regret,alpha_sync,coherence,error\n");
        for row in rows { csv.push_str(row); csv.push('\n'); }
        fs::write(fname, &csv).unwrap();
        println!("📄 {} ({} rows)", fname, rows.len());
    }

    println!();
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║           STRESS TEST COMPLETE                              ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");
}
