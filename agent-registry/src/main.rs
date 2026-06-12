//! Agent d'orchestration central — PHASE 5.
//!
//! Cible : prouver que SoulSystem (9023) est le hub central qui wire :
//! - L'orchestrateur mesh (9020) : 6 organes HNN routés
//! - 8 services mesh secondaires (memori, blackboard, pacemaker, etc.)
//! - 12 services tiers (ollama, omniclaw, onaeu, turboquant, etc.)
//! - 12 organes HNN (reason, perception, affect, language, integration, reflex, etc.)
//! - 8 agents externes (AVID, openevolve, synergie, brain, soul-neural, etc.)
//! - Le moteur Forge (évolution algorithmique, port 7890 si service)
//!
//! Usage :
//!   agent_registry once      # affiche un rapport snapshot
//!   agent_registry watch     # rapport toutes les 30s

use serde_json::Value;
use std::time::{Duration, Instant};

const SOULSYSTEM_URL: &str = "http://127.0.0.1:9023";
const ORCHESTRATOR_URL: &str = "http://127.0.0.1:9020";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("once");

    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║  SoulSystem Agent Registry (PHASE 5)                    ║");
    println!("║  Hub central du mesh — wire-up vérifié                  ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");

    match mode {
        "once" => run_snapshot().await?,
        "watch" => loop {
            run_snapshot().await?;
            tokio::time::sleep(Duration::from_secs(30)).await;
        },
        _ => {
            eprintln!("usage: agent_registry [once|watch]");
            std::process::exit(1);
        }
    }
    Ok(())
}

async fn run_snapshot() -> anyhow::Result<()> {
    let start = Instant::now();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()?;

    // 1. SoulSystem hubs
    let hubs = fetch_soulsystem_hubs(&client).await;
    println!("  ╭─ SoulSystem :9023 (hub central) ─────────────────────");
    for (name, status) in &hubs {
        let ok = status.starts_with("OK");
        println!(
            "  │  {:<10} {}",
            name,
            if ok {
                format!("✅ {}", truncate(status, 60))
            } else {
                format!("❌ {}", truncate(status, 60))
            }
        );
    }
    println!("  │");

    // 2. Orchestrator mesh
    let mesh = fetch_orchestrator_mesh(&client).await;
    println!("  ╰─ Orchestrator :9020 (mesh) ─────────────────────────");
    println!("     {}", mesh);

    // 3. Test inter-bridge : forward un ping de l'orch vers SoulSystem
    let fwd = forward_orch_to_soulsystem(&client).await;
    println!("\n  ╭─ Wire-up cross-bridge ──────────────────────────────");
    println!("  │  orch→soul forward : {}", fwd);

    println!("\n  ══ snapshot complet en {:.2?} ══\n", start.elapsed());
    Ok(())
}

async fn fetch_soulsystem_hubs(client: &reqwest::Client) -> Vec<(String, String)> {
    let mut results = Vec::new();

    // Bridges status (5 hard-coded)
    if let Ok(v) = get_json(client, &format!("{SOULSYSTEM_URL}/api/bridges/status")).await {
        if let Some(obj) = v.as_object() {
            for (k, v) in obj {
                results.push((k.clone(), v.as_str().unwrap_or("?").to_string()));
            }
        }
    }

    // Mesh
    if let Ok(v) = get_json(client, &format!("{SOULSYSTEM_URL}/api/bridges/mesh")).await {
        let count = v.get("count").and_then(|x| x.as_u64()).unwrap_or(0);
        results.push(("mesh".into(), format!("OK: {count} services")));
    }

    // Services
    if let Ok(v) = get_json(client, &format!("{SOULSYSTEM_URL}/api/bridges/services")).await {
        let count = v.get("count").and_then(|x| x.as_u64()).unwrap_or(0);
        results.push(("services".into(), format!("OK: {count} services")));
    }

    // Organs
    if let Ok(v) = get_json(client, &format!("{SOULSYSTEM_URL}/api/bridges/organs")).await {
        let v1 = v.get("v1_count").and_then(|x| x.as_u64()).unwrap_or(0);
        let v2 = v.get("v2_count").and_then(|x| x.as_u64()).unwrap_or(0);
        results.push(("organs".into(), format!("OK: {v1} v1 + {v2} v2")));
    }

    // Health
    if let Ok(v) = get_json(client, &format!("{SOULSYSTEM_URL}/health")).await {
        let ver = v.get("version").and_then(|x| x.as_str()).unwrap_or("?");
        results.push(("health".into(), format!("OK: v{ver}")));
    }

    results
}

async fn fetch_orchestrator_mesh(client: &reqwest::Client) -> String {
    match get_json(client, &format!("{ORCHESTRATOR_URL}/api/mesh/status")).await {
        Ok(v) => {
            let online = v.get("online").and_then(|x| x.as_u64()).unwrap_or(0);
            let total = v.get("total_brains").and_then(|x| x.as_u64()).unwrap_or(0);
            let total_n = v.get("total_N").and_then(|x| x.as_u64()).unwrap_or(0);
            format!("OK: {online}/{total} brains online, total_N = {total_n}")
        }
        Err(e) => format!("ERR: {e}"),
    }
}

async fn forward_orch_to_soulsystem(client: &reqwest::Client) -> String {
    // Demande au SoulSystem (9023) de relayer une query vers l'orch (9020) via /api/bridges/probe
    match client
        .post(format!("{SOULSYSTEM_URL}/api/bridges/probe"))
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            let v: Value = resp.json().await.unwrap_or(serde_json::json!({}));
            let keys: Vec<String> = v
                .as_object()
                .map(|o| o.keys().cloned().collect())
                .unwrap_or_default();
            format!("HTTP {status} — {} keys aggregated", keys.len())
        }
        Err(e) => format!("ERR: {e}"),
    }
}

async fn get_json(client: &reqwest::Client, url: &str) -> anyhow::Result<Value> {
    let resp = client.get(url).send().await?;
    let v: Value = resp.json().await?;
    Ok(v)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}
