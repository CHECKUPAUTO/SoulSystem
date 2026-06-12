// ==========================================================================
// stress_semantic.rs — Stress Test Long-Terme du SemanticStore V5
// ==========================================================================

use chronos_agent::memory::{AtemporalMemory, VectorIndex};
use rand::Rng;
use std::time::Instant;

const D_LATENT: usize = 16;
const EPISODIC_CAP: usize = 128;
const TOTAL_STEPS: usize = 10_000;
const REPORT_EVERY: usize = 1_000;
const SEARCH_K: usize = 5;

struct SemanticMetrics {
    total_insertions: usize,
    total_reinforcements: usize,
    total_searches: usize,
    search_time_us: Vec<f64>,
    prototype_drift: Vec<f64>,
    memory_alpha: Vec<f64>,
    alpha_collapses: usize,
}

fn main() {
    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║      STRESS SEMANTIC — Mémoire Long-Terme V5           ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
    println!();
    println!("  D_LATENT={}, EPISODIC_CAP={}, STEPS={}", D_LATENT, EPISODIC_CAP, TOTAL_STEPS);
    println!();

    let mut memory = AtemporalMemory::new(D_LATENT, EPISODIC_CAP);
    let mut rng = rand::thread_rng();
    let mut metrics = SemanticMetrics {
        total_insertions: 0,
        total_reinforcements: 0,
        total_searches: 0,
        search_time_us: Vec::with_capacity(TOTAL_STEPS / 10),
        prototype_drift: Vec::new(),
        memory_alpha: Vec::with_capacity(TOTAL_STEPS),
        alpha_collapses: 0,
    };

    let start = Instant::now();

    // --- Phase 1 : Remplissage ---
    println!("  📦 Phase 1 : Remplissage ({} steps)...", TOTAL_STEPS / 2);

    for step in 0..TOTAL_STEPS / 2 {
        let cluster_id = step % 5;
        let latent: Vec<f64> = (0..D_LATENT)
            .map(|i| {
                let center = (cluster_id as f64 * 0.3) + (i as f64 * 0.01);
                center + rng.gen::<f64>() * 0.1
            })
            .collect();

        let before = memory.semantic.index.len();
        memory.observe(&latent);
        let after = memory.semantic.index.len();
        if after > before { metrics.total_insertions += 1; } else { metrics.total_reinforcements += 1; }

        metrics.memory_alpha.push(memory.alpha_sync);
        if memory.alpha_sync < 0.01 { metrics.alpha_collapses += 1; }

        if step % 100 == 0 {
            let t0 = Instant::now();
            let _ = memory.semantic.index.search(&latent, SEARCH_K);
            metrics.search_time_us.push(t0.elapsed().as_micros() as f64);
            metrics.total_searches += 1;
        }

        if step % REPORT_EVERY == 0 && step > 0 {
            print!("\r    {} / {} | alpha={:.4} | semantic_size={}",
                step, TOTAL_STEPS, memory.alpha_sync, memory.semantic.index.len());
        }
    }

    println!();
    println!("  ✅ Phase 1 : {:.2}s — semantic_size={}", start.elapsed().as_secs_f64(), memory.semantic.index.len());
    println!();

    // --- Phase 2 : Stress ---
    println!("  🌪️  Phase 2 : Bruit + drift ({} steps)...", TOTAL_STEPS / 2);

    for step in 0..TOTAL_STEPS / 2 {
        let abs_step = TOTAL_STEPS / 2 + step;

        let latent: Vec<f64> = if step % 4 == 0 {
            (0..D_LATENT).map(|_| rng.gen::<f64>() * 2.0 - 1.0).collect()
        } else if step % 4 == 1 {
            let drift = step as f64 * 0.001;
            (0..D_LATENT).map(|i| (0.3 + drift) + (i as f64 * 0.01)).collect()
        } else {
            let cluster_id = (step / 3) % 5;
            (0..D_LATENT)
                .map(|i| (cluster_id as f64 * 0.3) + (i as f64 * 0.01) + rng.gen::<f64>() * 0.05)
                .collect()
        };

        memory.observe(&latent);
        metrics.memory_alpha.push(memory.alpha_sync);
        if memory.alpha_sync < 0.01 { metrics.alpha_collapses += 1; }

        if step % 100 == 0 {
            let t0 = Instant::now();
            let results = memory.semantic.index.search(&latent, SEARCH_K);
            metrics.search_time_us.push(t0.elapsed().as_micros() as f64);
            metrics.total_searches += 1;
            if let Some((_, sim)) = results.first() {
                metrics.prototype_drift.push((1.0 - sim).abs());
            }
        }

        if step % REPORT_EVERY == 0 && step > 0 {
            print!("\r    {} / {} (stress) | alpha={:.4} | semantic_size={}",
                abs_step, TOTAL_STEPS, memory.alpha_sync, memory.semantic.index.len());
        }
    }

    println!();
    println!();

    // --- Rapport ---
    let total_elapsed = start.elapsed();
    let total_strength: f64 = memory.semantic.strengths.iter().sum();
    let entropy: f64 = if total_strength > 0.0 {
        -memory.semantic.strengths.iter()
            .map(|s| { let p = s / total_strength; if p > 0.0 { p * p.ln() } else { 0.0 } })
            .sum::<f64>()
    } else { 0.0 };

    // Compute summary stats
    let avg_alpha = if !metrics.memory_alpha.is_empty() {
        metrics.memory_alpha.iter().sum::<f64>() / metrics.memory_alpha.len() as f64
    } else { 0.0 };
    let min_alpha = metrics.memory_alpha.iter().cloned().fold(1.0f64, f64::min);
    let alpha_below = metrics.memory_alpha.iter().filter(|&&a| a < 0.01).count();
    let avg_drift = if !metrics.prototype_drift.is_empty() {
        metrics.prototype_drift.iter().sum::<f64>() / metrics.prototype_drift.len() as f64
    } else { 0.0 };
    let max_drift = metrics.prototype_drift.iter().cloned().fold(0.0f64, f64::max);
    let avg_search = if !metrics.search_time_us.is_empty() {
        metrics.search_time_us.iter().sum::<f64>() / metrics.search_time_us.len() as f64
    } else { 0.0 };
    let max_search = metrics.search_time_us.iter().cloned().fold(0.0f64, f64::max);

    // Search time drift
    let (search_first, search_second) = if metrics.search_time_us.len() >= 4 {
        let mid = metrics.search_time_us.len() / 2;
        let f = metrics.search_time_us[..mid].iter().sum::<f64>() / mid as f64;
        let s = metrics.search_time_us[mid..].iter().sum::<f64>() / (metrics.search_time_us.len() - mid) as f64;
        (f, s)
    } else { (avg_search, avg_search) };

    let collapse_ratio = alpha_below as f64 / metrics.memory_alpha.len().max(1) as f64;

    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║               RAPPORT STRESS SEMANTIC                   ║");
    println!("╠═══════════════════════════════════════════════════════════╣");
    println!("  ⏱  Durée:                {:.2}s", total_elapsed.as_secs_f64());
    println!("  🧠 SemanticStore:         {}", memory.semantic.index.len());
    println!("  📦 EpisodicStore:         {}", memory.episodic.index.len());
    println!("  ➕ Insertions est.:       {}", metrics.total_insertions);
    println!("  🔄 Reinforcements est.:   {}", metrics.total_reinforcements);
    println!("  🔍 Searches:              {}", metrics.total_searches);
    println!("  ⏱  Search (avg/max):      {:.1}µs / {:.1}µs", avg_search, max_search);
    println!("  📈 Search drift:           {:.1}µs → {:.1}µs", search_first, search_second);
    println!("  🧠 Alpha sync (avg/min):   {:.4} / {:.4}", avg_alpha, min_alpha);
    println!("  🧠 Alpha < 0.01:           {} / {} ({:.1}%)", alpha_below, metrics.memory_alpha.len(), collapse_ratio * 100.0);
    println!("  📏 Drift (avg/max):        {:.4} / {:.4}", avg_drift, max_drift);
    println!("  💪 Strengths (avg/max):    {:.2} / {:.0}",
        memory.semantic.strengths.iter().sum::<f64>() / memory.semantic.strengths.len().max(1) as f64,
        memory.semantic.strengths.iter().cloned().fold(0.0f64, f64::max));
    println!("  🎲 Strength entropy:       {:.4}", entropy);
    println!("╠═══════════════════════════════════════════════════════════╣");

    let verdict = if collapse_ratio > 0.1 {
        "⚠️  ALPHA COLLAPSE TROP FRÉQUENT"
    } else if memory.semantic.index.len() >= 5000 {
        "⚠️  SATURATION BRUTE FORCE"
    } else if avg_alpha < 0.5 {
        "⚠️  ALPHA MOYEN TROP BAS"
    } else if avg_drift > 0.3 {
        "⚠️  DÉRIVE PROTOTYPE"
    } else {
        "✅  MÉMOIRE STABLE"
    };
    println!("║  🎯 {}", verdict);
    println!("╚═══════════════════════════════════════════════════════════╝");
}
