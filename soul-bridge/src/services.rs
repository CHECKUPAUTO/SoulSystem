// Pont services-bridge — multiplexe 12 services tiers du serveur
//
// Couvre TOUS les services actifs sur le serveur qui ne sont pas déjà
// gérés par les autres bridges (organes, mesh, openevolve, etc.).
//
// Services multiplexés :
//   - omniclaw         :9091  Orchestrateur Rust (brain, mesh, msa, turboquant)
//   - onaeu            :7878  AVID-like (Apex Engine) — web exploration/vision
//   - ollama           :11434 LLM inference server (62 modèles disponibles)
//   - turboquant-proxy :11435 Proxy trading Turboquant
//   - soulbridge       :11436 OpenAI↔Ollama proxy
//   - nats-server      :4222  NATS messaging (protocole TCP natif)
//   - crowdsec         :6060/8083 Cybersecurity agent
//   - mirofish-router  :7470  Mirofish Ollama router
//   - super-tool       :8085  Super-tool server (engines + tools)
//   - qmd-mcp          :8181  QMD MCP server
//   - sl13-monolith    :9045  HNN Monolith (Hamiltonian symplectic)
//   - novnc            :9060  noVNC web interface
//
// Communication bidirectionnelle :
//   - aller : query status, list models, lire state, browse endpoint
//   - retour : post message (NATS), generate (ollama), call (omniclaw)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Définition d'un service tiers.
#[derive(Debug, Clone, Copy)]
pub struct ServiceDef {
    pub name: &'static str,
    pub port: u16,
    pub health_path: &'static str,
    pub kind: &'static str,
}

pub const SERVICES: &[ServiceDef] = &[
    ServiceDef {
        name: "omniclaw",
        port: 9091,
        health_path: "/api/health",
        kind: "rust-orchestrator",
    },
    ServiceDef {
        name: "onaeu",
        port: 7878,
        health_path: "/health",
        kind: "avid-like",
    },
    ServiceDef {
        name: "ollama",
        port: 11434,
        health_path: "/api/version",
        kind: "llm-inference",
    },
    ServiceDef {
        name: "turboquant-proxy",
        port: 11435,
        health_path: "/api/version",
        kind: "trading-proxy",
    },
    ServiceDef {
        name: "soulbridge",
        port: 11436,
        health_path: "/health",
        kind: "openai-proxy",
    },
    ServiceDef {
        name: "nats",
        port: 4222,
        health_path: "/varz",
        kind: "messaging",
    },
    ServiceDef {
        name: "crowdsec",
        port: 8083,
        health_path: "/v1/usage",
        kind: "cybersecurity",
    },
    ServiceDef {
        name: "mirofish-router",
        port: 7470,
        health_path: "/health",
        kind: "llm-router",
    },
    ServiceDef {
        name: "super-tool",
        port: 8085,
        health_path: "/health",
        kind: "tool-server",
    },
    ServiceDef {
        name: "qmd-mcp",
        port: 8181,
        health_path: "/",
        kind: "mcp-server",
    },
    ServiceDef {
        name: "sl13-monolith",
        port: 9045,
        health_path: "/health",
        kind: "hnn-monolith",
    },
    ServiceDef {
        name: "novnc",
        port: 9060,
        health_path: "/",
        kind: "vnc-web",
    },
];

/// Status runtime d'un service.
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

/// Client multiplexe pour les services tiers.
#[derive(Clone)]
pub struct ServicesClient {
    client: reqwest::Client,
    statuses: Arc<RwLock<HashMap<String, ServiceStatus>>>,
}

impl ServicesClient {
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

    /// Sonde tous les services tiers.
    pub async fn probe_all(&self) -> HashMap<String, ServiceStatus> {
        let mut tasks = Vec::new();
        for svc in SERVICES {
            let client = self.client.clone();
            let name = svc.name.to_string();
            let port = svc.port;
            let kind = svc.kind.to_string();
            let health_path = svc.health_path.to_string();
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
                let url = format!("http://127.0.0.1:{}{}", port, health_path);
                if let Ok(resp) = client.get(&url).send().await {
                    if resp.status().is_success() {
                        if let Ok(v) = resp.json::<serde_json::Value>().await {
                            status.version =
                                v.get("version").and_then(|x| x.as_str()).map(String::from);
                            status.reachable = true;
                            status.detail = Some(v);
                        } else {
                            // Plaintext response (Ollama "Ollama is running")
                            status.reachable = true;
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
            "🌐 services-bridge: {}/{} tier services reachable",
            reachable,
            results.len()
        );
        results
    }

    /// Lit le status mis en cache.
    pub async fn status(&self) -> HashMap<String, ServiceStatus> {
        self.statuses.read().await.clone()
    }

    /// Appelle un endpoint arbitraire (aller).
    pub async fn call(
        &self,
        service_name: &str,
        path: &str,
        method: reqwest::Method,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let svc = SERVICES
            .iter()
            .find(|s| s.name == service_name)
            .ok_or_else(|| anyhow::anyhow!("unknown service: {service_name}"))?;
        let url = format!("http://127.0.0.1:{}{}", svc.port, path);

        let mut req = self.client.request(method, &url);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req.send().await.context("services call")?;
        let v: serde_json::Value = resp.json().await.context("services json")?;
        Ok(v)
    }

    // ── Actions spécialisées ────────────────────────────────────────────

    /// Liste les modèles Ollama disponibles (aller).
    pub async fn ollama_list_models(&self) -> Result<Vec<String>> {
        let v: serde_json::Value = self
            .call("ollama", "/api/tags", reqwest::Method::GET, None)
            .await?;
        let models = v
            .get("models")
            .and_then(|m| m.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("name").and_then(|n| n.as_str()))
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();
        Ok(models)
    }

    /// Demande une génération à Ollama (aller-retour).
    pub async fn ollama_generate(&self, model: &str, prompt: &str) -> Result<serde_json::Value> {
        self.call(
            "ollama",
            "/api/generate",
            reqwest::Method::POST,
            Some(serde_json::json!({
                "model": model,
                "prompt": prompt,
                "stream": false,
            })),
        )
        .await
    }

    /// Appelle l'API omniclaw /api/status (aller).
    pub async fn omniclaw_status(&self) -> Result<serde_json::Value> {
        self.call("omniclaw", "/api/status", reqwest::Method::GET, None)
            .await
    }

    /// Appelle l'API onaeu (AVID-like) pour crawl (aller-retour).
    pub async fn onaeu_crawl(&self, url: &str) -> Result<serde_json::Value> {
        self.call(
            "onaeu",
            "/api/scout/crawl",
            reqwest::Method::POST,
            Some(serde_json::json!({
                "url": url,
            })),
        )
        .await
    }

    /// Liste les outils du super-tool (aller).
    pub async fn supertool_list(&self) -> Result<serde_json::Value> {
        self.call("super-tool", "/tools", reqwest::Method::GET, None)
            .await
    }

    /// Lit l'état du sl13-monolith (HNN, aller).
    pub async fn monolith_health(&self) -> Result<serde_json::Value> {
        self.call("sl13-monolith", "/health", reqwest::Method::GET, None)
            .await
    }
}

/// Initialise le bridge.
pub async fn init() -> Result<ServicesClient> {
    let client = ServicesClient::connect().await?;
    info!("🌐 services-bridge initialized (12 tier services)");
    Ok(client)
}
