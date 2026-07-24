use std::time::Duration;

use cervo::{
    config::CervoConfig, create_labeled_test_data, AmplifyTransform, Cortex, Data,
    IdentityTransform, MemoryConfig, PipelineConfig, PipelineTransform, ReverseTransform, SwarmBus,
    Transformation, TransformationRegistry, UnitHandle,
};
use tracing_subscriber::EnvFilter;

fn separator(title: &str) {
    println!("\n═══════════════════════════════════════════════════");
    println!("  {title}");
    println!("═══════════════════════════════════════════════════\n");
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,cervo=warn")),
        )
        .init();
    println!(
        "╔══════════════════════════════════════════════════╗\n\
         ║     CERVO v2 — Écosystème Auto-Évolutif         ║\n\
         ║   Swarm · Mémoire · Fitness · Pipeline · Cortex ║\n\
         ╚══════════════════════════════════════════════════╝\n"
    );

    separator("1. MÉMOIRE HIÉRARCHIQUE (store · recall · consolidate · decay)");
    demo_memory().await;

    separator("2. PIPELINE — construction déclarative (config → registre)");
    demo_pipeline().await;

    separator("3. SWARM — traitement de flux, échange & adoption inter-unités");
    demo_swarm_and_evolution().await;

    separator("4. CORTEX — supervision : process_all, santé, arrêt coordonné");
    demo_cortex().await;

    separator("5. CONFIGURATION — validation + persistance disque");
    demo_custom_config().await;

    println!("\n✓ Tous les systèmes CERVO v2 sont opérationnels.");
}

async fn demo_memory() {
    use cervo::memory::Memory;

    let config = MemoryConfig {
        active_capacity: 4,
        consolidation_threshold: 1.5,
        decay_rate: 0.15,
        initial_strength: 1.0,
    };

    let mut mem = Memory::new(&config);
    println!("  active_capacity=4, consolidation_threshold=1.5\n");

    for i in 0..6 {
        let strength = if i >= 3 { 2.0 } else { 1.0 };
        mem.store_with_priority(Data::new(vec![i as u8; 4]), strength);
        let (active, long) = mem.snapshot();
        println!(
            "  store item {i} (strength={strength:.0})  │ active={:<2}  long_term={}",
            active.len(),
            long.len()
        );
    }

    // recall : la mémoire écrite est réellement relue (renforcement).
    println!("\n  ── recall ──");
    match mem.recall(&[5u8; 4]) {
        Some(d) => println!(
            "  recall([5;4]) → trouvé, {} octets (renforcé)",
            d.payload.len()
        ),
        None => println!("  recall([5;4]) → absent"),
    }

    println!("\n  ── consolidation ──");
    mem.consolidate();
    let (active, long) = mem.snapshot();
    println!(
        "  après consolidate  │ active={}  long_term={}",
        active.len(),
        long.len()
    );

    println!("\n  ── decay ──");
    mem.decay();
    let (active, long) = mem.snapshot();
    println!(
        "  après decay  │ active={}  long_term={}",
        active.len(),
        long.len()
    );
}

async fn demo_pipeline() {
    // Construction pilotée par configuration via le registre de transformations.
    let registry = TransformationRegistry::default();
    let config =
        PipelineConfig::single("reverse").add_stage("amplify", serde_json::json!({ "factor": 2 }));

    let pipeline = match PipelineTransform::from_config(&config, &registry) {
        Ok(p) => p,
        Err(e) => {
            println!("  Erreur de construction du pipeline : {e}");
            return;
        }
    };

    println!("  Pipeline (config-driven) : {}", pipeline.describe());
    println!("  Stable : {}\n", pipeline.is_stable());

    let input = vec![Data::new(vec![1, 2, 3]), Data::new(vec![4, 5, 6])];

    match pipeline.transform(&input) {
        Ok(output) => {
            println!(
                "  Entrée : {:?}",
                input.iter().map(|d| &d.payload).collect::<Vec<_>>()
            );
            println!(
                "  Sortie : {:?}",
                output.iter().map(|d| &d.payload).collect::<Vec<_>>()
            );
        }
        Err(e) => println!("  Erreur : {e}"),
    }
}

async fn demo_swarm_and_evolution() {
    let swarm = SwarmBus::new(128);

    let unit_a = UnitHandle::with_swarm(
        Box::new(IdentityTransform),
        swarm.clone(),
        &CervoConfig::default(),
    );
    let unit_b = UnitHandle::with_swarm(
        Box::new(ReverseTransform),
        swarm.clone(),
        &CervoConfig::default(),
    );
    // Une unité autonome (constructeur direct) branchée au même bus.
    let unit_c = UnitHandle::with_swarm(
        Box::new(AmplifyTransform { factor: 2 }),
        swarm.clone(),
        &CervoConfig::default(),
    );

    // Laisser les acteurs s'abonner au bus.
    tokio::time::sleep(Duration::from_millis(20)).await;
    println!(
        "  3 unités (identity, reverse, amplify) connectées au bus (subs={})\n",
        swarm.subscriber_count()
    );

    // ── Traitement d'un flux de données réel, diffusé sur le swarm ──
    let stream = create_labeled_test_data(8, 3, "flux-entrant");
    println!("  ── Traitement d'un flux ({} messages) ──", stream.len());
    let out_a = unit_a.process_data(stream.clone()).await.unwrap();
    let _ = unit_b.process_data(stream.clone()).await.unwrap();
    let _ = unit_c.process_data(stream).await.unwrap();
    tokio::time::sleep(Duration::from_millis(40)).await;
    println!(
        "    A a produit {} sorties tamponnées (source_unit renseignée : {})",
        out_a.len(),
        out_a
            .first()
            .map(|d| d.metadata.source_unit.is_some())
            .unwrap_or(false)
    );

    // ── Rounds de mutation : les unités partagent/adoptent les bons algos ──
    println!("\n  ── Mutations + échange inter-unités ──");
    for i in 1..=6 {
        let r_a = unit_a.mutate().await;
        let r_b = unit_b.mutate().await;
        let r_c = unit_c.mutate().await;
        // laisser circuler les annonces/partages
        tokio::time::sleep(Duration::from_millis(10)).await;
        if i % 2 == 0 {
            println!(
                "    round {i}: A[{}] B[{}] C[{}]",
                mark(r_a.accepted),
                mark(r_b.accepted),
                mark(r_c.accepted)
            );
        }
    }

    // ── Échange de santé : quelques cycles → HealthReport/Heartbeat circulent ──
    println!("\n  ── Échange de santé (cycles) ──");
    for _ in 0..3 {
        unit_a.cycle().await;
        unit_b.cycle().await;
        unit_c.cycle().await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    for (name, u) in [("A", &unit_a), ("B", &unit_b), ("C", &unit_c)] {
        let s = u.get_state().await;
        println!(
            "  Unité {name}: algo={}, traité={}, adopté={}, pairs_connus={}, rejets={}",
            s.algorithm_name, s.processed_count, s.adopted_count, s.known_peers, s.rejected_count
        );
        if let Some(reason) = &s.last_rejection {
            println!("    dernier rejet : {reason}");
        }
    }
}

async fn demo_cortex() {
    // try_new : la configuration est validée avant tout démarrage.
    let mut cortex = match Cortex::try_new(CervoConfig::default()) {
        Ok(c) => c,
        Err(e) => {
            println!("  Config invalide : {e}");
            return;
        }
    };

    let _ = cortex.spawn(Box::new(IdentityTransform)).await;
    let _ = cortex.spawn(Box::new(ReverseTransform)).await;
    let _ = cortex.spawn(Box::new(AmplifyTransform { factor: 2 })).await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    println!(
        "  {} unités supervisées (bus subs={})\n",
        cortex.unit_count(),
        cortex.swarm().subscriber_count()
    );

    // Traitement collectif d'un jeu de données.
    let dataset = create_labeled_test_data(16, 4, "batch");
    let results = cortex.process_all(dataset).await;
    let ok = results.iter().filter(|(_, r)| r.is_ok()).count();
    println!(
        "  process_all : {ok}/{} unités ont traité le batch",
        results.len()
    );

    // Cycles globaux (émettent santé + battements sur le bus).
    for _ in 0..3 {
        cortex.tick_all().await;
    }
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Mutations pour amorcer la fitness.
    for id in cortex
        .report()
        .await
        .units
        .iter()
        .map(|u| u.id)
        .collect::<Vec<_>>()
    {
        if let Some(h) = cortex.get(&id) {
            let _ = h.mutate().await;
        }
    }
    cortex.tick_all().await;
    tokio::time::sleep(Duration::from_millis(20)).await;

    let report = cortex.report().await;
    println!("\n  ── Rapport Cortex ──");
    println!("    Unités              : {}", report.unit_count);
    println!("    Données traitées    : {}", report.total_processed);
    println!("    Algos adoptés (pairs): {}", report.total_adopted);
    println!("    Mutations totales   : {}", report.total_mutations);
    println!("    Santé par unité (rsi, sat, mém active, pairs connus) :");
    for u in &report.units {
        println!(
            "      {}… rsi={:5.1} sat={:.2} mém={} pairs={}",
            &u.id.to_string()[..8],
            u.rsi_index,
            u.saturation,
            u.memory_active_count,
            u.known_peers
        );
    }
    println!("    Fitness par famille :");
    for (family, attempts, successes, fitness) in &report.evolution_summary {
        println!("      {family:>10}: {successes}/{attempts} = {fitness:.2}");
    }

    // Arrêt coordonné du swarm (via le bus).
    cortex.broadcast_shutdown().await;
    println!(
        "\n  Arrêt coordonné diffusé — {} unités restantes.",
        cortex.unit_count()
    );
}

async fn demo_custom_config() {
    let mut config = CervoConfig::default();
    config.sandbox = cervo::config::SandboxConfig {
        timeout: Duration::from_millis(10),
        max_output_bytes: 2_048,
        max_divergence: 50.0,
    };
    config.memory.active_capacity = 20;
    config.mutation.exploration_rate = 0.8;

    // Validation explicite.
    match config.validate() {
        Ok(()) => println!("  ✓ config valide"),
        Err(e) => {
            println!("  ✗ config invalide : {e}");
            return;
        }
    }

    // Persistance disque : save → load (roundtrip réel).
    let path = std::env::temp_dir().join("cervo_config_demo.json");
    if config.save(&path).is_ok() {
        match CervoConfig::load(&path) {
            Ok(loaded) => println!(
                "  ✓ config persistée puis relue depuis {} (timeout={:?})",
                path.display(),
                loaded.sandbox.timeout
            ),
            Err(e) => println!("  ✗ relecture échouée : {e}"),
        }
        let _ = std::fs::remove_file(&path);
    }

    // Démonstration : sous une config serrée, une mutation instable est rejetée.
    let unit = UnitHandle::with_config(Box::new(IdentityTransform), &config);
    let report = unit.mutate().await;
    println!(
        "\n  Mutation sous config serrée : {}",
        if report.accepted {
            "✓ acceptée"
        } else {
            "✗ rejetée"
        }
    );
    if let Some(reason) = &report.rejection_reason {
        println!("    raison : {reason}");
    }
}

fn mark(ok: bool) -> &'static str {
    if ok {
        "✓"
    } else {
        "✗"
    }
}
