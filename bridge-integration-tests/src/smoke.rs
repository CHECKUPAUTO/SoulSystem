//! Smoke test des 8 bridges SoulSystem ↔ écosystème.
//!
//! Exécute `init()` ou `init() → probe_all()` sur chaque bridge en série
//! (pour éviter la saturation réseau) et affiche un tableau récapitulatif.
//!
//! Tous les bridges individuels (avid-bridge, brain-bridge, etc.) ont été
//! consolidés dans le crate `soul-bridge`. Les modules sont accessibles via
//! `soul_bridge::avid`, `soul_bridge::brain`, etc.

use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║  SoulSystem Bridges — Smoke Test (PHASE 2)              ║");
    println!("║  (unified via soul-bridge)                              ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");

    let total_start = Instant::now();
    let mut all_results: Vec<(&str, String, Duration, bool)> = Vec::new();

    // 1. AVID bridge — sync init
    {
        let start = Instant::now();
        let res = tokio::task::spawn_blocking(soul_bridge::avid::init).await;
        let (msg, ok) = match res {
            Ok(Ok(())) => ("AVID bridge init OK".to_string(), true),
            Ok(Err(e)) => (format!("init error: {e}"), false),
            Err(e) => (format!("join error: {e}"), false),
        };
        all_results.push(("avid-bridge", msg, start.elapsed(), ok));
    }

    // 2. OpenEvolve bridge — async init
    {
        let start = Instant::now();
        let res = tokio::time::timeout(Duration::from_secs(8), soul_bridge::openevolve::init()).await;
        let (msg, ok) = match res {
            Ok(Ok(_)) => ("openevolve-bridge init OK".to_string(), true),
            Ok(Err(e)) => (format!("init error: {e}"), false),
            Err(_) => ("timeout 8s".to_string(), false),
        };
        all_results.push(("openevolve-bridge", msg, start.elapsed(), ok));
    }

    // 3. mesh-bridge — async init → MeshClient → probe_all
    {
        let start = Instant::now();
        let res = tokio::time::timeout(Duration::from_secs(10), soul_bridge::mesh::init()).await;
        let (msg, ok) = match res {
            Ok(Ok(client)) => {
                let probed = client.probe_all().await;
                let up = probed.values().filter(|s| s.reachable).count();
                let total = probed.len();
                (format!("MeshClient OK, {up}/{total} services UP"), up > 0)
            }
            Ok(Err(e)) => (format!("init error: {e}"), false),
            Err(_) => ("timeout 10s".to_string(), false),
        };
        all_results.push(("mesh-bridge", msg, start.elapsed(), ok));
    }

    // 4. services-bridge
    {
        let start = Instant::now();
        let res = tokio::time::timeout(Duration::from_secs(12), soul_bridge::services::init()).await;
        let (msg, ok) = match res {
            Ok(Ok(client)) => {
                let probed = client.probe_all().await;
                let up = probed.values().filter(|s| s.reachable).count();
                let total = probed.len();
                (
                    format!("ServicesClient OK, {up}/{total} services UP"),
                    up > 0,
                )
            }
            Ok(Err(e)) => (format!("init error: {e}"), false),
            Err(_) => ("timeout 12s".to_string(), false),
        };
        all_results.push(("services-bridge", msg, start.elapsed(), ok));
    }

    // 5. organs-bridge
    {
        let start = Instant::now();
        let res = tokio::time::timeout(Duration::from_secs(10), soul_bridge::organs::init()).await;
        let (msg, ok) = match res {
            Ok(Ok(client)) => {
                let probed = client.probe_all().await;
                let up = probed.values().filter(|s| s.reachable).count();
                let total = probed.len();
                (format!("OrgansClient OK, {up}/{total} organes UP"), up > 0)
            }
            Ok(Err(e)) => (format!("init error: {e}"), false),
            Err(_) => ("timeout 10s".to_string(), false),
        };
        all_results.push(("organs-bridge", msg, start.elapsed(), ok));
    }

    // 6. brain-bridge
    {
        let start = Instant::now();
        let res = tokio::time::timeout(Duration::from_secs(8), soul_bridge::brain::init()).await;
        let (msg, ok) = match res {
            Ok(Ok(_)) => ("brain-bridge init OK".to_string(), true),
            Ok(Err(e)) => (format!("init error: {e}"), false),
            Err(_) => ("timeout 8s".to_string(), false),
        };
        all_results.push(("brain-bridge", msg, start.elapsed(), ok));
    }

    // 7. soul-neural-bridge
    {
        let start = Instant::now();
        let res = tokio::time::timeout(Duration::from_secs(8), soul_bridge::soul_neural::init()).await;
        let (msg, ok) = match res {
            Ok(Ok(_)) => ("soul-neural-bridge init OK".to_string(), true),
            Ok(Err(e)) => (format!("init error: {e}"), false),
            Err(_) => ("timeout 8s".to_string(), false),
        };
        all_results.push(("soul-neural-bridge", msg, start.elapsed(), ok));
    }

    // 8. synergie-bridge
    {
        let start = Instant::now();
        let res = tokio::time::timeout(Duration::from_secs(8), soul_bridge::synergie::init()).await;
        let (msg, ok) = match res {
            Ok(Ok(_)) => ("synergie-bridge init OK".to_string(), true),
            Ok(Err(e)) => (format!("init error: {e}"), false),
            Err(_) => ("timeout 8s".to_string(), false),
        };
        all_results.push(("synergie-bridge", msg, start.elapsed(), ok));
    }

    // 9. forge-bridge (le moteur évolutionnaire)
    {
        let start = Instant::now();
        let res = tokio::time::timeout(
            Duration::from_secs(10),
            tokio::task::spawn_blocking(|| {
                use forge_bridge::{binpack_demo::BinPacking, ForgeCampaign, ForgeConfig};
                let campaign = ForgeCampaign::new(
                    ForgeConfig {
                        generations: 3,
                        population: 12,
                        survivors: 2,
                        base_seed: 42,
                        max_reflection_retries: 2,
                        stagnation_threshold: 5,
                    },
                    BinPacking {
                        capacity: 1.0,
                        n_items: 30,
                        n_instances: 10,
                    },
                );
                let report = campaign.run();
                if report.history.is_empty() {
                    anyhow::bail!("aucun historique")
                }
                Ok::<(), anyhow::Error>(())
            }),
        )
        .await;
        let (msg, ok) = match res {
            Ok(Ok(Ok(()))) => ("forge-bridge evolution OK".to_string(), true),
            Ok(Ok(Err(e))) => (format!("forge error: {e}"), false),
            Ok(Err(e)) => (format!("join error: {e}"), false),
            Err(_) => ("timeout 10s".to_string(), false),
        };
        all_results.push(("forge-bridge", msg, start.elapsed(), ok));
    }

    // ─── Affichage récapitulatif ───
    println!();
    println!("  {:<22} {:<55} {:<10} OK", "Bridge", "Résultat", "Latence");
    println!(
        "  {} {} {} ──",
        "─".repeat(22),
        "─".repeat(55),
        "─".repeat(10),
    );
    for (name, msg, dur, ok) in &all_results {
        println!(
            "  {:<22} {:<55} {:<10} {}",
            name,
            msg,
            format!("{:.2?}", dur),
            if *ok { "✅" } else { "❌" }
        );
    }
    let passed = all_results.iter().filter(|(_, _, _, ok)| *ok).count();
    let total = all_results.len();
    println!();
    println!(
        "  ══ {passed}/{total} bridges opérationnels — total {:.2?} ══",
        total_start.elapsed()
    );
    if passed == total {
        println!("\n  🟢 TOUS LES BRIDGES SONT BRANCHÉS ET FONCTIONNELS.\n");
        std::process::exit(0);
    } else {
        println!("\n  🟡 {} bridge(s) à investiguer.\n", total - passed);
        std::process::exit(1);
    }
}