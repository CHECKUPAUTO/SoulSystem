//! PHASE 4 — Probe SoulSystem ↔ Orchestrator.
//!
//! Démontre que l'orchestrateur (9020) est joignable et que ses 6+ organes
//! sont dans le mesh. C'est le premier pont explicite SoulSystem → orchestrateur.

use std::time::Instant;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║  SoulSystem → Orchestrator Bridge (PHASE 4)             ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");

    let start = Instant::now();
    let client = orchestrator_bridge::OrchestratorClient::connect("http://127.0.0.1:9020");

    // 1. Health
    let h = client.health().await?;
    println!("  ✅ orchestrator /health : {}", h);

    // 2. Mesh status
    let s = client.mesh_status().await?;
    println!("\n  📊 mesh : {}/{} online, total_N = {}",
        s.online, s.total_brains, s.total_N);

    // 3. Brains (registered)
    let b = client.mesh_brains().await?;
    println!("\n  🧠 registered brains ({} total) :", b.total);
    let mut names: Vec<_> = b.brains.iter().collect();
    names.sort_by(|a, b| a.0.cmp(b.0));
    for (name, brain) in names {
        println!("     {:<10} port={:<6} version={:<5} spec={:?}",
            name, brain.port, brain.version, brain.speciality);
    }

    // 4. Test query (au cas où)
    println!("\n  💬 test query vers mesh...");
    let q = client.mesh_query("hello mesh, qui es-tu ?", Some(2)).await?;
    println!("     réponse : {}", q);

    println!("\n  ══ bridge OK en {:.2?} ══\n", start.elapsed());
    Ok(())
}
