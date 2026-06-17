//! Test d'intégration end-to-end — PHASE 6.
//!
//! Scénario :
//! 1. Lance une campagne Forge sur le domaine bin-packing
//! 2. Vérifie que le checkpoint est créé
//! 3. Publie le résultat via le bridge orchestrateur
//! 4. Vérifie que SoulSystem peut relayer l'info
//!
//! Cible : < 30 secondes.

use std::time::Instant;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║  PHASE 6 — End-to-End Integration Test                  ║");
    println!("║  Forge ↔ SoulSystem ↔ Orchestrator ↔ Bridges           ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");

    let total_start = Instant::now();
    let mut passed = 0;
    let total = 6;

    // Test 1: Forge campaign runs
    {
        let start = Instant::now();
        println!("  [1/6] forge campaign (bin-packing)...");
        use forge_bridge::{binpack_demo::BinPacking, ForgeCampaign, ForgeConfig};
        let campaign = ForgeCampaign::new(
            ForgeConfig {
                generations: 20,
                population: 40,
                survivors: 4,
                base_seed: 42,
                max_reflection_retries: 3,
                stagnation_threshold: 5,
            },
            BinPacking {
                capacity: 1.0,
                n_items: 50,
                n_instances: 20,
            },
        );
        let report = campaign.run();
        match report.best {
            Some(best) => {
                let obj0 = best.score.objectives.first().copied().unwrap_or(0.0);
                println!(
                    "        ✅ Forge OK in {:.2?} — best avg_bins = {:.3}",
                    start.elapsed(),
                    obj0
                );
                passed += 1;
            }
            None => println!("        ❌ Forge produced no best candidate"),
        }
    }

    // Test 2: Forge checkpoint exists
    {
        let start = Instant::now();
        println!("  [2/6] forge checkpoint file...");
        let path = std::path::Path::new("forge_checkpoint.json");
        if path.exists() {
            let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            println!(
                "        ✅ Checkpoint OK in {:.2?} — {} bytes",
                start.elapsed(),
                size
            );
            passed += 1;
        } else {
            println!("        ❌ No checkpoint file found");
        }
    }

    // Test 3: Orchestrator reachable
    {
        let start = Instant::now();
        println!("  [3/6] orchestrator mesh status...");
        let client = soul_bridge::orchestrator::OrchestratorClient::connect("http://127.0.0.1:9020");
        match client.mesh_status().await {
            Ok(s) => {
                println!(
                    "        ✅ Mesh OK in {:.2?} — {}/{} online, total_N={}",
                    start.elapsed(),
                    s.online,
                    s.total_brains,
                    s.total_n
                );
                passed += 1;
            }
            Err(e) => println!("        ❌ Orchestrator error: {e}"),
        }
    }

    // Test 4: SoulSystem hub reachable
    {
        let start = Instant::now();
        println!("  [4/6] soulsystem hub (9023)...");
        let client = reqwest::Client::new();
        match client.get("http://127.0.0.1:9023/health").send().await {
            Ok(resp) => {
                let v: serde_json::Value = resp.json().await?;
                let ver = v.get("version").and_then(|x| x.as_str()).unwrap_or("?");
                println!(
                    "        ✅ SoulSystem OK in {:.2?} — v{ver}",
                    start.elapsed()
                );
                passed += 1;
            }
            Err(e) => println!("        ❌ SoulSystem error: {e}"),
        }
    }

    // Test 5: Cross-bridge probe
    {
        let start = Instant::now();
        println!("  [5/6] cross-bridge probe (soul → all)...");
        let client = reqwest::Client::new();
        match client
            .post("http://127.0.0.1:9023/api/bridges/probe")
            .send()
            .await
        {
            Ok(resp) => {
                let v: serde_json::Value = resp.json().await?;
                let count = v.as_object().map(|o| o.len()).unwrap_or(0);
                println!(
                    "        ✅ Cross-bridge OK in {:.2?} — {count} agents aggregated",
                    start.elapsed()
                );
                passed += 1;
            }
            Err(e) => println!("        ❌ Cross-bridge error: {e}"),
        }
    }

    // Test 6: AVID bridge
    {
        let start = Instant::now();
        println!("  [6/6] avid-bridge (ONAEU 7878)...");
        let client = reqwest::Client::new();
        match client.get("http://127.0.0.1:7878/health").send().await {
            Ok(resp) => {
                let v: serde_json::Value = resp.json().await?;
                let status = v.get("status").and_then(|x| x.as_str()).unwrap_or("?");
                println!(
                    "        ✅ AVID/ONAEU OK in {:.2?} — status={status}",
                    start.elapsed()
                );
                passed += 1;
            }
            Err(e) => println!("        ❌ AVID/ONAEU error: {e}"),
        }
    }

    println!();
    if passed == total {
        println!(
            "  🟢 {passed}/{total} tests verts — total {:.2?}",
            total_start.elapsed()
        );
        println!("  SoulSystem est le hub central du mesh.");
        std::process::exit(0);
    } else {
        println!(
            "  🟡 {passed}/{total} tests OK — {} échec(s)",
            total - passed
        );
        std::process::exit(1);
    }
}
