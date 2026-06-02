// Pont mesh-bridge — services mesh secondaires non couverts par organs-bridge
//
// Services multiplexés :
//   - rag-turbo-bridge        :9070  (moteur RAG)
//   - rowboat-soullink-bridge :9071  (proxy OAuth/stream)
//   - soullink-moe            :9072  (Mixture of Experts middleware)
//   - soullink-pacemaker      :9073  (1Hz homeostatic pacemaker, free-energy)
//   - soullink-blackboard     :9074  (Global Workspace Theory, coalitions)
//   - soullink-memori         :9075  (mémoire épisodique, CortexSynapse backend)
//   - soullink-v14            :9095  (orchestration v14)
//   - soullink-voice          :9050  (TTS/STT voice)
//
// Communication bidirectionnelle (aller-retour) :
//   - aller : query status, lire coalitions, recall memori, list entries
//   - retour : post stimulus, push entry, store memory, spawn task

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Définition d'un service mesh.
#[derive(Debug, Clone, Copy)]
pub struct ServiceDef {
    pub name: &'static str,
    pub port: u16,
    pub kind: &'static str, // "passthrough" | "status-only" | "tts"
}

pub const MESH_SERVICES: &[ServiceDef] = &[
    ServiceDef { name: "rag_turbo",  port: 9070, kind: "passthrough" },
    ServiceDef { name: "rowboat",    port: 9071, kind: "passthrough" },
    ServiceDef { name: "moe",        port: 9072, kind: "passthrough" },
    ServiceDef { name: "pacemaker",  port: 9073, kind: "status-only" },
    ServiceDef { name: "blackboard", port: 9074, kind: "status-only" },
    ServiceDef { name: "memori",     port: 9075, kind: "status-only" },
    ServiceDef { name: "v14",        port: 9095, kind: "passthrough" },
    ServiceDef { name: "voice",      port: 9050, kind: "tts" },
];

/// Status runtime d'un service mesh.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub name: String,
    pub port: u16,
    pub kind: String,
    pub reachable: bool,
    pub version: Option<String>,
    pub last_check: Option<chrono::DateTime<chrono::Utc>>,
    pub detail: Option<serde_json::Value>,
}

/// Client multiplexe pour les services mesh.
#[derive(Clone)]
pub struct MeshClient {
    client: reqwest::Client,
    statuses: Arc<RwLock<HashMap<String, ServiceStatus>>>,
}

impl MeshClient {
    /// Crée un client et sonde tous les services.
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

    /// Sonde tous les services mesh.
    pub async fn probe_all(&self) -> HashMap<String, ServiceStatus> {
        let mut tasks = Vec::new();
        for svc in MESH_SERVICES {
            let client = self.client.clone();
            let name = svc.name.to_string();
            let port = svc.port;
            let kind = svc.kind.to_string();
            tasks.push(tokio::spawn(async move {
                let mut status = ServiceStatus {
                    name: name.clone(),
                    port,
                    kind: kind.clone(),
                    reachable: false,
                    version: None,
                    last_check: Some(chrono::Utc::now()),
                    detail: None,
                };

                // Try /health (passthrough + tts) or /api/status (status-only)
                let url_health = format!("http://127.0.0.1:{}/health", port);
                let url_status = format!("http://127.0.0.1:{}/api/status", port);

                if let Ok(resp) = client.get(&url_health).send().await {
                    if resp.status().is_success() {
                        if let Ok(v) = resp.json::<serde_json::Value>().await {
                            status.version = v.get("version").and_then(|x| x.as_str()).map(String::from);
                            status.reachable = true;
                            status.detail = Some(v);
                        }
                    }
                } else if let Ok(resp) = client.get(&url_status).send().await {
                    if resp.status().is_success() {
                        if let Ok(v) = resp.json::<serde_json::Value>().await {
                            status.version = v.get("version").and_then(|x| x.as_str()).map(String::from);
                            status.reachable = true;
                            status.detail = Some(v);
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
        info!("🕸️ mesh-bridge: {}/{} mesh services reachable", reachable, results.len());
        results
    }

    /// Lit le status mis en cache.
    pub async fn status(&self) -> HashMap<String, ServiceStatus> {
        self.statuses.read().await.clone()
    }

    /// Appelle un endpoint arbitraire sur un service mesh (aller).
    pub async fn call(
        &self,
        service_name: &str,
        path: &str,
        method: reqwest::Method,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let svc = MESH_SERVICES
            .iter()
            .find(|s| s.name == service_name)
            .ok_or_else(|| anyhow::anyhow!("unknown service: {service_name}"))?;
        let url = format!("http://127.0.0.1:{}{}", svc.port, path);

        let mut req = self.client.request(method, &url);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req.send().await.context("mesh call")?;
        let v: serde_json::Value = resp.json().await.context("mesh json")?;
        Ok(v)
    }

    // ── Actions spécialisées ────────────────────────────────────────────

    /// Lit les coalitions actives du blackboard (aller).
    pub async fn blackboard_coalitions(&self) -> Result<serde_json::Value> {
        self.call("blackboard", "/api/coalitions", reqwest::Method::GET, None).await
    }

    /// Lit le status détaillé du pacemaker (aller).
    pub async fn pacemaker_status(&self) -> Result<serde_json::Value> {
        self.call("pacemaker", "/api/status", reqwest::Method::GET, None).await
    }

    /// Lit le status du memori (aller).
    pub async fn memori_status(&self) -> Result<serde_json::Value> {
        self.call("memori", "/api/status", reqwest::Method::GET, None).await
    }

    /// Pousse un stimulus vers le pacemaker (retour).
    pub async fn pacemaker_decide(&self, signal: &str) -> Result<serde_json::Value> {
        self.call("pacemaker", "/api/decide", reqwest::Method::POST, Some(serde_json::json!({ "signal": signal })))
            .await
    }
}

/// Initialise le bridge : crée le client et log l'état.
pub async fn init() -> Result<MeshClient> {
    let client = MeshClient::connect().await?;
    info!("🕸️ mesh-bridge initialized (8 mesh services: rag, rowboat, moe, pacemaker, blackboard, memori, v14, voice)");
    Ok(client)
}
