// Pont openevolve-bridge — client HTTP réel vers openevolve.service (port 7879)
//
// Communique dans les deux sens :
//   - aller : query status, get best program, trigger evolve step, fetch metrics
//   - retour : push feedback (score), publish program à openevolve
//
// Toutes les méthodes sont async et utilisent reqwest avec rustls.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Status runtime de la connexion openevolve.
#[derive(Debug, Clone)]
pub struct BridgeStatus {
    pub reachable: bool,
    pub version: Option<String>,
    pub last_check: chrono::DateTime<chrono::Utc>,
    pub best_score: f64,
    pub total_programs: u64,
}

/// Client openevolve — parle à `http://127.0.0.1:7879` (openevolve.service).
#[derive(Clone)]
pub struct OpenEvolveClient {
    base_url: String,
    client: reqwest::Client,
    status: Arc<RwLock<BridgeStatus>>,
}

#[derive(Debug, Deserialize)]
struct HealthResponse {
    service: String,
    version: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct StatusResponse {
    best_generation: u64,
    best_id: String,
    best_score: f64,
    total_programs: u64,
    config: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct MetricsResponse {
    build_profile: String,
    service: String,
    version: String,
}

impl OpenEvolveClient {
    /// Crée un client et sonde immédiatement l'état du service.
    pub async fn connect(base_url: impl Into<String>) -> Result<Self> {
        let base_url = base_url.into();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .context("build reqwest client")?;

        let status = BridgeStatus {
            reachable: false,
            version: None,
            last_check: chrono::Utc::now(),
            best_score: 0.0,
            total_programs: 0,
        };

        let me = Self {
            base_url,
            client,
            status: Arc::new(RwLock::new(status)),
        };

        me.probe().await.ok();
        Ok(me)
    }

    /// Sonde l'état du service openevolve (health + status).
    pub async fn probe(&self) -> Result<BridgeStatus> {
        let health: HealthResponse = self
            .client
            .get(format!("{}/v1/health", self.base_url))
            .send()
            .await
            .context("openevolve health request")?
            .json()
            .await
            .context("openevolve health json")?;

        let status: StatusResponse = self
            .client
            .get(format!("{}/v1/status", self.base_url))
            .send()
            .await
            .context("openevolve status request")?
            .json()
            .await
            .context("openevolve status json")?;

        // /v1/metrics peut renvoyer du text/plain Prometheus, pas du JSON strict.
        let version = match self
            .client
            .get(format!("{}/v1/metrics", self.base_url))
            .send()
            .await
        {
            Ok(resp) => {
                let text = resp.text().await.unwrap_or_default();
                if let Ok(m) = serde_json::from_str::<MetricsResponse>(&text) {
                    m.version
                } else {
                    health.version.clone()
                }
            }
            Err(_) => health.version.clone(),
        };

        let s = BridgeStatus {
            reachable: true,
            version: Some(version.clone()),
            last_check: chrono::Utc::now(),
            best_score: status.best_score,
            total_programs: status.total_programs,
        };
        *self.status.write().await = s.clone();
        info!(
            "🧬 OpenEvolve bridge OK: v{} programs={} best_score={}",
            version, s.total_programs, s.best_score
        );
        Ok(s)
    }

    /// Lit le status mis en cache (rapide, sans appel HTTP).
    pub async fn cached_status(&self) -> BridgeStatus {
        self.status.read().await.clone()
    }

    /// Déclenche une étape d'évolution avec un bout de code (aller).
    pub async fn evolve_step(&self, code: &str, language: &str) -> Result<serde_json::Value> {
        let body = serde_json::json!({ "code": code, "language": language });
        let resp: serde_json::Value = self
            .client
            .post(format!("{}/v1/evolve", self.base_url))
            .json(&body)
            .send()
            .await
            .context("openevolve evolve POST")?
            .json()
            .await
            .context("openevolve evolve json")?;
        Ok(resp)
    }

    /// Pousse un score de feedback (retour) vers openevolve.
    pub async fn submit_feedback(&self, program_id: &str, score: f64) -> Result<serde_json::Value> {
        let body = serde_json::json!({
            "program_id": program_id,
            "score": score,
        });
        let resp: serde_json::Value = self
            .client
            .post(format!("{}/v1/feedback", self.base_url))
            .json(&body)
            .send()
            .await
            .context("openevolve feedback POST")?
            .json()
            .await
            .context("openevolve feedback json")?;
        Ok(resp)
    }
}

/// Initialise le bridge : crée le client et log l'état.
pub async fn init() -> Result<OpenEvolveClient> {
    let url =
        std::env::var("OPENEVOLVE_URL").unwrap_or_else(|_| "http://127.0.0.1:7879".to_string());
    let client = OpenEvolveClient::connect(url).await?;
    info!("🧬 openevolve-bridge initialized (HTTP client)");
    Ok(client)
}
