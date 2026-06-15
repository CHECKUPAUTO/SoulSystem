//! Route: Spawn

use crate::state::AppState;
use axum::{extract::State, Json};
use serde_json::{json, Value};
use tokio::process::Command;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};

/// Route POST /api/mesh/spawn
pub async fn route_spawn(State(state): State<AppState>, Json(body): Json<Value>) -> Json<Value> {
    let domain = body
        .get("domain")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if domain.is_empty() {
        return Json(json!({"ok": false, "error": "domain requis"}));
    }

    if state.brain_exists(&domain) {
        return Json(json!({
            "ok": false,
            "error": format!("Brain '{}' existe déjà", domain)
        }));
    }

    // Trouver un port libre
    let used_ports: Vec<u16> = state.get_used_ports();
    let mut port = 9016u16;
    while used_ports.contains(&port) {
        port += 1;
    }

    let brain_dir = state.brain_dir.clone();
    let domain_c = domain.clone();
    let _domain_for_config = domain.clone();
    let _speciality = body
        .get("speciality")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![domain.clone()]);

    state.increment_spawn_count();

    // Spawn en background
    tokio::spawn(async move {
        info!("[SPAWN] Lancement Brain-{} sur :{}", domain_c, port);

        let result = Command::new("python3")
            .arg(format!("{}/brain_v12.py", brain_dir))
            .arg("--brain")
            .arg(&domain_c)
            .spawn();

        match result {
            Ok(_child) => {
                // Attendre le démarrage
                sleep(Duration::from_secs(8)).await;

                // Vérifier que le cerveau est accessible
                let url = format!("http://localhost:{}/api/stats", port);
                let client = reqwest::Client::builder()
                    .timeout(Duration::from_secs(2))
                    .build()
                    .unwrap();

                for _ in 0..6 {
                    if let Ok(Ok(resp)) =
                        tokio::time::timeout(Duration::from_secs(2), client.get(&url).send()).await
                    {
                        if resp.status().is_success() {
                            info!("[SPAWN] ✅ Brain-{} actif sur :{}", domain_c, port);
                            return;
                        }
                    }
                    sleep(Duration::from_secs(5)).await;
                }
                warn!("[SPAWN] ⚠️ Brain-{} ne répond pas", domain_c);
            }
            Err(e) => {
                error!("[SPAWN] ❌ Erreur: {}", e);
            }
        }
    });

    Json(json!({
        "ok": true,
        "domain": domain,
        "port": port,
        "msg": format!("Spawn Brain-{} en cours sur :{}", domain, port),
        "check": "/api/mesh/status dans 30s",
    }))
}
