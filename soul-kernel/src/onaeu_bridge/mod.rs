//! ONAÉ-U Bridge — injection de mutations

use serde_json::json;
use tracing::{info, warn};

pub struct OnaeuBridge {
    url: String,
    client: reqwest::Client,
}

impl OnaeuBridge {
    pub fn new(url: &str, client: reqwest::Client) -> Self {
        Self {
            url: url.trim_end_matches('/').to_string(),
            client,
        }
    }

    pub async fn mutate(&self, action: &str) -> Result<String, String> {
        let resp = self
            .client
            .post(format!("{}/mutate", self.url))
            .json(&json!({"action": action, "params": {}}))
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        if resp.status().is_success() {
            info!("✅ ONAÉ-U mutation '{}' injected", action);
            Ok(format!("Mutation '{}' injected", action))
        } else {
            warn!("❌ ONAÉ-U mutation '{}' failed: {}", action, resp.status());
            Err(format!("Mutation failed: {}", resp.status()))
        }
    }

    #[allow(dead_code)]
    pub async fn state(&self) -> Result<serde_json::Value, String> {
        let resp = self
            .client
            .get(format!("{}/state", self.url))
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        if resp.status().is_success() {
            resp.json().await.map_err(|e| format!("Parse error: {}", e))
        } else {
            Err(format!("Status: {}", resp.status()))
        }
    }
}
