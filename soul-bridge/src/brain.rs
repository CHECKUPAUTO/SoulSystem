// Pont brain-bridge — client HTTP réel vers les 6 organes HNN (ports 9010-9015)
//
// Communique dans les deux sens :
//   - aller : probe status, learn(topic), eval math, run SSM step
//   - retour : push stimulus, trigger evolve, submit feedback sur health
//
// Le bridge multiplexe 6 organes HNN (science, mind, engineer, crypto, creative, meta)
// pour répartir la charge et exploiter le mesh neural.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Noms des 6 organes HNN (par convention SoulLink).
pub const ORGANS: &[(&str, u16)] = &[
    ("science", 9010),
    ("mind", 9011),
    ("engineer", 9012),
    ("crypto", 9013),
    ("creative", 9014),
    ("meta", 9015),
];

/// Status d'un organe.
#[derive(Debug, Clone)]
pub struct OrganStatus {
    pub name: String,
    pub port: u16,
    pub reachable: bool,
    pub energy_avg: f64,
    pub evolution_generation: u64,
    pub neurons: u64,
}

/// Client brain — parle aux 6 organes HNN en parallèle.
#[derive(Clone)]
pub struct BrainClient {
    client: reqwest::Client,
    organs: Arc<RwLock<Vec<OrganStatus>>>,
}

impl BrainClient {
    /// Crée un client et sonde l'état des 6 organes.
    pub async fn connect() -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .context("build reqwest client")?;
        let me = Self {
            client,
            organs: Arc::new(RwLock::new(Vec::new())),
        };
        me.probe_all().await;
        Ok(me)
    }

    /// Sonde tous les organes en parallèle.
    pub async fn probe_all(&self) -> Vec<OrganStatus> {
        let mut tasks = Vec::new();
        for (name, port) in ORGANS {
            let url = format!("http://127.0.0.1:{}/api/health", port);
            let client = self.client.clone();
            let name = name.to_string();
            let port = *port;
            tasks.push(tokio::spawn(async move {
                let (energy_avg, evolution_generation, neurons) =
                    match client.get(&url).send().await {
                        Ok(resp) => match resp.json::<serde_json::Value>().await {
                            Ok(v) => (
                                v.get("energy_avg").and_then(|x| x.as_f64()).unwrap_or(0.0),
                                v.get("evolution_generation")
                                    .and_then(|x| x.as_u64())
                                    .unwrap_or(0),
                                v.get("neurons").and_then(|x| x.as_u64()).unwrap_or(0),
                            ),
                            Err(_) => (0.0, 0, 0),
                        },
                        Err(_) => (0.0, 0, 0),
                    };
                OrganStatus {
                    name,
                    port,
                    reachable: neurons > 0 || energy_avg > 0.0,
                    energy_avg,
                    evolution_generation,
                    neurons,
                }
            }));
        }

        let mut results = Vec::new();
        for task in tasks {
            if let Ok(s) = task.await {
                results.push(s);
            }
        }

        *self.organs.write().await = results.clone();
        let reachable = results.iter().filter(|s| s.reachable).count();
        info!(
            "🧠 brain-bridge: {}/{} HNN organs reachable",
            reachable,
            results.len()
        );
        results
    }

    /// Lit le status mis en cache.
    pub async fn status(&self) -> Vec<OrganStatus> {
        self.organs.read().await.clone()
    }

    /// Apprend un topic (aller) sur l'organe spécifié (par défaut science).
    pub async fn learn(&self, topic: &str, organ: &str) -> Result<serde_json::Value> {
        let port = ORGANS
            .iter()
            .find(|(n, _)| *n == organ)
            .map(|(_, p)| *p)
            .unwrap_or(9010);
        let resp: serde_json::Value = self
            .client
            .post(format!("http://127.0.0.1:{}/api/learn", port))
            .json(&serde_json::json!({ "topic": topic }))
            .send()
            .await
            .context("brain learn POST")?
            .json()
            .await
            .context("brain learn json")?;
        Ok(resp)
    }

    /// Évalue une expression math (aller) sur l'organe engineer.
    pub async fn math_eval(&self, expr: &str) -> Result<serde_json::Value> {
        let resp: serde_json::Value = self
            .client
            .post("http://127.0.0.1:9012/api/math/eval")
            .json(&serde_json::json!({ "expr": expr, "vars": {} }))
            .send()
            .await
            .context("brain math eval POST")?
            .json()
            .await
            .context("brain math eval json")?;
        Ok(resp)
    }

    /// Pousse un stimulus (retour) sur l'organe meta.
    pub async fn stimulate(&self, intensity: f64) -> Result<serde_json::Value> {
        let resp: serde_json::Value = self
            .client
            .post("http://127.0.0.1:9015/api/evolve/trigger")
            .json(&serde_json::json!({ "auto_evolve": true, "intensity": intensity }))
            .send()
            .await
            .context("brain stimulate POST")?
            .json()
            .await
            .context("brain stimulate json")?;
        Ok(resp)
    }
}

/// Initialise le bridge : crée le client et log l'état.
pub async fn init() -> Result<BrainClient> {
    let client = BrainClient::connect().await?;
    info!("🧠 brain-bridge initialized (6 HNN organs, HTTP client)");
    Ok(client)
}
