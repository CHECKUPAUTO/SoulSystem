/// Pont AVID ↔ SoulSystem — connexion réelle via HTTP à l'API AVID
/// Les types Scout et Vision sont de vrais clients HTTP qui parlent à avid-server
use anyhow::Result;

pub fn init() -> Result<()> {
    // Port 8080 = OWNCLOUD (réservé). AVID a migré vers ONAEU :7878 par défaut.
    // Le port peut être surchargé via AVID_ENDPOINT (utile pour un AVID distant).
    let endpoint =
        std::env::var("AVID_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:7878".to_string());
    tracing::info!(
        "🧬 AVID bridge initialized (HTTP client to {} — set AVID_ENDPOINT to override)",
        endpoint
    );
    Ok(())
}

/// Client AVID Scout — explore le web via l'API AVID
pub struct Scout {
    base_url: String,
    client: reqwest::Client,
}

impl Default for Scout {
    fn default() -> Self {
        Self {
            // Port 8080 = OWNCLOUD (réservé). AVID a migré vers ONAEU :7878 par défaut.
            base_url: std::env::var("AVID_ENDPOINT")
                .unwrap_or_else(|_| "http://127.0.0.1:7878".to_string()),
            client: reqwest::Client::new(),
        }
    }
}

impl Scout {
    pub async fn crawl(&self, url: &str) -> Result<()> {
        let resp = self
            .client
            .post(format!("{}/api/scout/crawl", self.base_url))
            .json(&serde_json::json!({"url": url}))
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("AVID Scout crawl failed: {}", resp.status());
        }
        tracing::info!("🔍 AVID Scout crawled: {}", url);
        Ok(())
    }
}

/// Client AVID Vision — perception visuelle via l'API AVID
pub struct Vision {
    #[allow(dead_code)]
    base_url: String,
    #[allow(dead_code)]
    client: reqwest::Client,
}

impl Default for Vision {
    fn default() -> Self {
        Self {
            // Port 8080 = OWNCLOUD (réservé). AVID a migré vers ONAEU :7878 par défaut.
            base_url: std::env::var("AVID_ENDPOINT")
                .unwrap_or_else(|_| "http://127.0.0.1:7878".to_string()),
            client: reqwest::Client::new(),
        }
    }
}
