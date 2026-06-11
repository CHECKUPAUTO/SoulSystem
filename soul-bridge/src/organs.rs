// Pont organs-bridge — multiplexe 12 organes HNN (Hamiltonian Neural Engine)
//
// Deux générations d'organes cohabitent :
//   - V1 (6 organes) : reasoning, integration, perception, affect, reflex, language
//     sur ports 5031-5036, exposent des APIs de stimuli et rewards
//   - V2 (6 organes) : foresight, homeostasis, creativity, social, validation, autonomy
//     sur ports 9040-9046, exposent /api/stats avec concepts_count, hz, etc.
//
// Communication bidirectionnelle :
//   - aller : query reasoning, perceive input, stimulate reflex, check homeostasis
//   - retour : push stimulus, post reward, log conversation-stimulus

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Définition d'un organe : nom, port, endpoints d'API.
#[derive(Debug, Clone, Copy)]
pub struct OrganDef {
    pub name: &'static str,
    pub port: u16,
    pub generation: u8, // 1 = v1 API, 2 = v2 API
}

pub const V1_ORGANS: &[OrganDef] = &[
    OrganDef {
        name: "reasoning",
        port: 5031,
        generation: 1,
    },
    OrganDef {
        name: "integration",
        port: 5032,
        generation: 1,
    },
    OrganDef {
        name: "perception",
        port: 5033,
        generation: 1,
    },
    OrganDef {
        name: "affect",
        port: 5034,
        generation: 1,
    },
    OrganDef {
        name: "reflex",
        port: 5035,
        generation: 1,
    },
    OrganDef {
        name: "language",
        port: 5036,
        generation: 1,
    },
];

pub const V2_ORGANS: &[OrganDef] = &[
    OrganDef {
        name: "foresight",
        port: 9040,
        generation: 2,
    },
    OrganDef {
        name: "homeostasis",
        port: 9041,
        generation: 2,
    },
    OrganDef {
        name: "creativity",
        port: 9042,
        generation: 2,
    },
    OrganDef {
        name: "social",
        port: 9043,
        generation: 2,
    },
    OrganDef {
        name: "validation",
        port: 9044,
        generation: 2,
    },
    OrganDef {
        name: "autonomy",
        port: 9046,
        generation: 2,
    },
];

/// Status runtime d'un organe.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrganStatus {
    pub name: String,
    pub port: u16,
    pub generation: u8,
    pub reachable: bool,
    pub version: Option<String>,
    pub last_check: Option<chrono::DateTime<chrono::Utc>>,
    pub stats: Option<serde_json::Value>,
}

/// Client multiplexe pour 12 organes HNN.
#[derive(Clone)]
pub struct OrgansClient {
    client: reqwest::Client,
    statuses: Arc<RwLock<HashMap<String, OrganStatus>>>,
}

impl OrgansClient {
    /// Crée un client et sonde les 12 organes en parallèle.
    pub async fn connect() -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .context("build reqwest client")?;

        let me = Self {
            client,
            statuses: Arc::new(RwLock::new(HashMap::new())),
        };
        me.probe_all().await;
        Ok(me)
    }

    /// Sonde tous les organes (V1 + V2) en parallèle.
    pub async fn probe_all(&self) -> HashMap<String, OrganStatus> {
        let mut tasks = Vec::new();
        for organ in V1_ORGANS.iter().chain(V2_ORGANS.iter()) {
            let url_root = format!("http://127.0.0.1:{}/", organ.port);
            let url_stats = format!("http://127.0.0.1:{}/api/stats", organ.port);
            let client = self.client.clone();
            let name = organ.name.to_string();
            let port = organ.port;
            let gen = organ.generation;
            tasks.push(tokio::spawn(async move {
                let mut status = OrganStatus {
                    name: name.clone(),
                    port,
                    generation: gen,
                    reachable: false,
                    version: None,
                    last_check: Some(chrono::Utc::now()),
                    stats: None,
                };
                if let Ok(resp) = client.get(&url_root).send().await {
                    if let Ok(v) = resp.json::<serde_json::Value>().await {
                        status.version =
                            v.get("version").and_then(|x| x.as_str()).map(String::from);
                        status.reachable = true;
                    }
                }
                if gen == 2 {
                    if let Ok(resp) = client.get(&url_stats).send().await {
                        if let Ok(s) = resp.json::<serde_json::Value>().await {
                            status.stats = Some(s);
                        }
                    }
                }
                (name, status)
            }));
        }

        let mut results = HashMap::new();
        for task in tasks {
            if let Ok((name, status)) = task.await {
                results.insert(name, status);
            }
        }
        *self.statuses.write().await = results.clone();

        let reachable = results.values().filter(|s| s.reachable).count();
        info!(
            "🧠 organs-bridge: {}/{} organs reachable",
            reachable,
            results.len()
        );
        results
    }

    /// Lit le status mis en cache.
    pub async fn status(&self) -> HashMap<String, OrganStatus> {
        self.statuses.read().await.clone()
    }

    /// Appelle un endpoint arbitraire sur un organe (aller).
    pub async fn call(
        &self,
        organ_name: &str,
        path: &str,
        method: reqwest::Method,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let organ = V1_ORGANS
            .iter()
            .chain(V2_ORGANS.iter())
            .find(|o| o.name == organ_name)
            .ok_or_else(|| anyhow::anyhow!("unknown organ: {organ_name}"))?;
        let url = format!("http://127.0.0.1:{}{}", organ.port, path);

        let mut req = self.client.request(method, &url);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req.send().await.context("organ call")?;
        let v: serde_json::Value = resp.json().await.context("organ json")?;
        Ok(v)
    }

    // ── Actions V1 (aller-retour) ────────────────────────────────────────

    /// Demande à l'organe `reasoning` de raisonner sur un input (aller).
    pub async fn reason(&self, input: &str) -> Result<serde_json::Value> {
        self.call(
            "reasoning",
            "/api/reason",
            reqwest::Method::POST,
            Some(serde_json::json!({ "input": input })),
        )
        .await
    }

    /// Envoie un stimulus à l'organe `perception` (aller).
    pub async fn perceive(&self, stimulus: &str) -> Result<serde_json::Value> {
        self.call(
            "perception",
            "/api/perceive",
            reqwest::Method::POST,
            Some(serde_json::json!({ "input": stimulus })),
        )
        .await
    }

    /// Stimule l'organe `reflex` (aller-retour).
    pub async fn stimulate_reflex(&self, signal: &str) -> Result<serde_json::Value> {
        self.call(
            "reflex",
            "/api/stimulate",
            reqwest::Method::POST,
            Some(serde_json::json!({ "signal": signal })),
        )
        .await
    }

    /// Envoie un reward à un organe (retour) — push vers l'organe.
    pub async fn reward(&self, organ_name: &str, value: f64) -> Result<serde_json::Value> {
        self.call(
            organ_name,
            "/api/reward",
            reqwest::Method::POST,
            Some(serde_json::json!({ "value": value })),
        )
        .await
    }

    // ── Actions V2 (aller-retour) ────────────────────────────────────────

    /// Lit les stats d'un organe V2 (aller).
    pub async fn stats(&self, organ_name: &str) -> Result<serde_json::Value> {
        self.call(organ_name, "/api/stats", reqwest::Method::GET, None)
            .await
    }
}

/// Initialise le bridge : crée le client et log l'état.
pub async fn init() -> Result<OrgansClient> {
    let client = OrgansClient::connect().await?;
    info!("🧠 organs-bridge initialized (12 HNN organs: 6 v1 + 6 v2)");
    Ok(client)
}
