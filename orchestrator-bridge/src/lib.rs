//! Pont SoulSystem (9023) ↔ Orchestrator (9020)
//!
//! Permet à SoulSystem de :
//! - Consulter l'état du mesh (quels organes sont UP, énergie, spikes)
//! - Router une question vers le bon organe
//! - Signaler sa propre santé à l'orchestrateur
//!
//! Usage :
//! ```no_run
//! use orchestrator_bridge::OrchestratorClient;
//! let client = OrchestratorClient::connect("http://127.0.0.1:9020").await?;
//! let status = client.mesh_status().await?;
//! ```

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct OrchestratorClient {
    base_url: String,
    client: reqwest::Client,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrganStatus {
    pub port: u16,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub hz: f64,
    #[serde(default)]
    pub energy_avg: f64,
    #[serde(default)]
    pub spikes: u64,
    #[serde(default)]
    pub N: u64,
    #[serde(default)]
    pub attractor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshStatus {
    pub online: usize,
    pub total_brains: usize,
    pub total_N: u64,
    pub mesh: HashMap<String, OrganStatus>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrganBrain {
    pub port: u16,
    #[serde(default)]
    pub spawned_by: String,
    #[serde(default)]
    pub speciality: Vec<String>,
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainsList {
    pub total: usize,
    pub brains: HashMap<String, OrganBrain>,
}

impl OrchestratorClient {
    pub fn connect(base_url: impl Into<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("reqwest client");
        Self { base_url: base_url.into(), client }
    }

    /// GET /health — health check de l'orchestrateur.
    pub async fn health(&self) -> Result<Value> {
        let resp = self.client.get(format!("{}/health", self.base_url)).send().await
            .context("orchestrator /health")?;
        let v: Value = resp.json().await.context("orchestrator /health json")?;
        Ok(v)
    }

    /// GET /api/mesh/status — état complet du mesh.
    pub async fn mesh_status(&self) -> Result<MeshStatus> {
        let resp = self.client.get(format!("{}/api/mesh/status", self.base_url)).send().await
            .context("orchestrator /api/mesh/status")?;
        let v: Value = resp.json().await?;
        let s: MeshStatus = serde_json::from_value(v)?;
        Ok(s)
    }

    /// GET /api/mesh/brains — liste des organes instanciés.
    pub async fn mesh_brains(&self) -> Result<BrainsList> {
        let resp = self.client.get(format!("{}/api/mesh/brains", self.base_url)).send().await
            .context("orchestrator /api/mesh/brains")?;
        let v: Value = resp.json().await?;
        let b: BrainsList = serde_json::from_value(v)?;
        Ok(b)
    }

    /// POST /api/mesh/query — route une question vers l'organe le plus pertinent.
    pub async fn mesh_query(&self, question: &str, top_k: Option<usize>) -> Result<Value> {
        let body = serde_json::json!({
            "query": question,
            "top_k": top_k.unwrap_or(3),
        });
        let resp = self.client.post(format!("{}/api/mesh/query", self.base_url))
            .json(&body).send().await
            .context("orchestrator /api/mesh/query")?;
        let v: Value = resp.json().await?;
        Ok(v)
    }

    /// Ping périodique du mesh — log les organes qui ne répondent pas.
    pub async fn heartbeat_summary(&self) -> Result<String> {
        let status = self.mesh_status().await?;
        let mut out = format!("mesh: {}/{} online, total_N={}\n",
            status.online, status.total_brains, status.total_N);
        let mut organs: Vec<_> = status.mesh.iter().collect();
        organs.sort_by(|a, b| a.0.cmp(b.0));
        for (name, s) in organs {
            out.push_str(&format!("  {:<10} port={:<6} state={:<10} hz={:.2} N={}\n",
                name, s.port, s.state, s.hz, s.N));
        }
        Ok(out)
    }
}

/// Init rapide — utilise ORCHESTRATOR_URL ou l'URL par défaut.
pub async fn init() -> Result<OrchestratorClient> {
    let base = std::env::var("ORCHESTRATOR_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:9020".to_string());
    let client = OrchestratorClient::connect(base);
    client.health().await.context("orchestrator health")?;
    Ok(client)
}
