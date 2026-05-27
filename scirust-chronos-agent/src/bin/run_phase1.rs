// ==========================================================================
// run_phase1.rs — Lance la Phase 1 Observation sur un vrai modèle GGUF
//
// Charge Llama-3.2-1B (ou tout autre GGUF), observe les latents,
// les stocke dans le SemanticStore, et analyse les clusters.
//
// Usage: cargo run --release --bin run_phase1
// ==========================================================================

use chronos_agent::llm_bridge::*;
use chronos_agent::memory::{AtemporalMemory, VectorIndex};

const D_LATENT: usize = 1024; // Qwen3 0.6B embedding dim (RTX 4060 compatible)
const EPISODIC_CAP: usize = 64;

fn main() {
    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║      PHASE 1 — Observation LLM Réel                    ║");
    println!("╚═══════════════════════════════════════════════════════════╝");
    println!();

    // Configuration pour Qwen3 0.6B (embeddings uniquement — 610 MB, tient dans 8GB VRAM)
    let config = LLMConfig {
        model_path: "/root/.cache/qmd/models/hf_ggml-org_qwen3-reranker-0.6b-q8_0.gguf".to_string(),
        tokenizer_path: String::new(), // intégré dans le GGUF
        d_model: D_LATENT,
        vocab_size: 151936,
        num_layers: 28,
        n_heads: 16,
        d_head: 64,
        device: candle_core::Device::Cpu,
        max_seq_len: 2048,
    };

    println!("  📦 Configuration:");
    println!("     Model:     {}", config.model_path);
    println!("     D_model:   {}", config.d_model);
    println!("     Vocab:     {}", config.vocab_size);
    println!("     Layers:    {}", config.num_layers);
    println!("     Device:    CPU");
    println!();

    // Initialisation
    let mut observer = LLMObserver::new(config);
    let mut memory = AtemporalMemory::new(D_LATENT, EPISODIC_CAP);

    // Phase 1.0 : Validation du fichier modèle
    let model_exists = std::path::Path::new(&observer.config().model_path).exists();
    println!("  🔍 Modèle présent: {}", if model_exists { "✅ OUI" } else { "❌ NON" });

    if !model_exists {
        eprintln!("  ⚠️  Modèle introuvable. Lancement en mode mock uniquement.");
        eprintln!("     Chemin vérifié: {}", observer.config().model_path);
    }

    // Charge (ou valide) le modèle
    match observer.load() {
        Ok(_) => println!("  ✅ Modèle chargé"),
        Err(e) => eprintln!("  ⚠️  Load: {}", e),
    }

    // Phase 1.1 : Observation de prompts variés
    println!();
    println!("  📝 Phase 1.1 — Observation de 10 prompts...");

    let prompts = vec![
        "Explain the concept of recursion in programming.",
        "Write a haiku about artificial intelligence.",
        "What is the capital of France?",
        "Solve: 2 + 2 = ?",
        "Summarize quantum computing in one sentence.",
        "Translate 'hello world' to French.",
        "What is the meaning of life?",
        "Write a short poem about Rust programming.",
        "Explain the difference between TCP and UDP.",
        "Why is the sky blue?",
    ];

    for (i, prompt) in prompts.iter().enumerate() {
        let latent = observer.observe(prompt).unwrap_or_else(|_| {
            // Fallback sur le mock si le modèle réel ne répond pas
            vec![0.0; D_LATENT]
        });

        memory.observe(&latent, None);

        // Stats de l'observation
        let energy: f64 = latent.iter().map(|x| x * x).sum::<f64>().sqrt();
        let max_val = latent.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let min_val = latent.iter().cloned().fold(f64::INFINITY, f64::min);
        let mean = latent.iter().sum::<f64>() / latent.len() as f64;

        println!("     [{:2}/{:2}] energy={:.2} range=[{:.2}, {:.2}] mean={:.4} α={:.4}",
            i + 1, prompts.len(), energy, min_val, max_val, mean, memory.alpha_sync);
    }

    // Phase 1.2 : Analyse des clusters
    println!();
    println!("  🔬 Phase 1.2 — Analyse des clusters...");

    let report = analyze_clusters(&observer);
    println!("     Clusters détectés:   {}", report.cluster_count);
    println!("     Cohérence intra:     {:.4}", report.intra_coherence);
    println!("     Séparation inter:    {:.4}", report.inter_separation);
    println!("     Ratio séparation:    {:.4}", report.separation_ratio);
    println!("     Observations:        {}", report.total_observations);
    println!();

    // Rapport mémoire
    println!("  🧠 Rapport mémoire:");
    println!("     SemanticStore:      {} entrées", memory.semantic.index.len());
    println!("     Alpha sync (avg):   {:.4}", memory.alpha_sync);
    println!("     Strengths (total):  {}", memory.semantic.prototypes.len());
    if !memory.semantic.prototypes.is_empty() {
        let total_s: f64 = memory.semantic.prototypes.iter().map(|p| p.strength).sum();
        let max_s = memory.semantic.prototypes.iter().map(|p| p.strength).fold(0.0f64, f64::max);
        println!("     Strengths (sum/max): {:.0} / {:.0}", total_s, max_s);
    }
    println!();

    // Verdict
    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║  VERDICT PHASE 1                                        ║");

    let verdict = if report.cluster_count >= 3 && report.intra_coherence > 0.5 {
        "✅  SYSTÈME PRÊT — Clusters bien formés, passer à Phase 2"
    } else if report.cluster_count >= 1 {
        "⚠️  CLUSTERS PARTIELS — Plus d'observations nécessaires"
    } else {
        "❌  AUCUN CLUSTER — Vérifier le mock ou le modèle"
    };

    println!("║  {}", verdict);
    println!("╚═══════════════════════════════════════════════════════════╝");
}
