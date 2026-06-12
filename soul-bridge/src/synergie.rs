// Pont synergie-bridge — client HTTP réel vers synergie (port 7460)
//
// Communique dans les deux sens :
//   - aller : list synergies, scan, query ecosystem
//   - retour : post feedback (approve/reject), force sync github
//
// synergie tourne comme daemon (synergie.service) et expose une API axum.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Status runtime de la connexion synergie.
#[derive(Debug, Clone)]
pub struct BridgeStatus {
    pub reachable: bool,
    pub version: Option<String>,
    pub synergies: u64,
    pub embed_cache: u64,
    pub last_check: chrono::DateTime<chrono::Utc>,
}

/// Client synergie — parle à `http://127.0.0.1:7460` (synergie.service).
#[derive(Clone)]
pub struct SynergieClient {
    base_url: String,
    client: reqwest::Client,
    status: Arc<RwLock<BridgeStatus>>,
}

#[derive(Debug, Deserialize)]
struct HealthResponse {
    status: String,
    version: String,
    counts: CountsResponse,
}

#[derive(Debug, Deserialize)]
struct CountsResponse {
    synergies: u64,
    #[serde(rename = "embed_cache")]
    embed_cache: u64,
}

#[derive(Debug, Serialize)]
pub struct FeedbackRequest {
    pub status: String, // "approved" | "rejected" | "skipped"
    pub note: Option<String>,
}

impl SynergieClient {
    /// Crée un client et sonde l'état du service.
    pub async fn connect(base_url: impl Into<String>) -> Result<Self> {
        let base_url = base_url.into();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .context("build reqwest client")?;

        let status = BridgeStatus {
            reachable: false,
            version: None,
            synergies: 0,
            embed_cache: 0,
            last_check: chrono::Utc::now(),
        };

        let me = Self {
            base_url,
            client,
            status: Arc::new(RwLock::new(status)),
        };

        me.probe().await.ok();
        Ok(me)
    }

    /// Sonde l'état du service synergie.
    pub async fn probe(&self) -> Result<BridgeStatus> {
        let resp: HealthResponse = self
            .client
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .context("synergie health")?
            .json()
            .await
            .context("synergie health json")?;

        let s = BridgeStatus {
            reachable: resp.status == "ok",
            version: Some(resp.version),
            synergies: resp.counts.synergies,
            embed_cache: resp.counts.embed_cache,
            last_check: chrono::Utc::now(),
        };
        *self.status.write().await = s.clone();
        info!(
            "🔗 synergie-bridge OK: v{} synergies={} embed_cache={}",
            s.version.as_deref().unwrap_or("?"),
            s.synergies,
            s.embed_cache
        );
        Ok(s)
    }

    /// Lit le status mis en cache.
    pub async fn cached_status(&self) -> BridgeStatus {
        self.status.read().await.clone()
    }

    /// Liste les synergies (aller).
    pub async fn list_synergies(&self, limit: u32) -> Result<Vec<serde_json::Value>> {
        let resp: Vec<serde_json::Value> = self
            .client
            .get(format!("{}/synergies?limit={}", self.base_url, limit))
            .send()
            .await
            .context("synergie list")?
            .json()
            .await
            .context("synergie list json")?;
        Ok(resp)
    }

    /// Déclenche un scan de l'écosystème (aller).
    pub async fn trigger_scan(&self) -> Result<String> {
        let resp: serde_json::Value = self
            .client
            .post(format!("{}/scan", self.base_url))
            .json(&serde_json::json!({}))
            .send()
            .await
            .context("synergie scan POST")?
            .json()
            .await
            .context("synergie scan json")?;
        Ok(resp
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string())
    }

    /// Pousse un feedback (retour) sur une synergie.
    pub async fn post_feedback(&self, id: &str, fb: FeedbackRequest) -> Result<serde_json::Value> {
        let resp: serde_json::Value = self
            .client
            .post(format!("{}/feedback/{}", self.base_url, id))
            .json(&fb)
            .send()
            .await
            .context("synergie feedback POST")?
            .json()
            .await
            .context("synergie feedback json")?;
        Ok(resp)
    }
}

/// Initialise le bridge : crée le client et log l'état.
pub async fn init() -> Result<SynergieClient> {
    let url = std::env::var("SYNERGIE_URL").unwrap_or_else(|_| "http://127.0.0.1:7460".to_string());
    let client = SynergieClient::connect(url).await?;
    info!("🔗 synergie-bridge initialized (HTTP client)");
    Ok(client)
}
