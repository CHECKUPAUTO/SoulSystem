//! Memory — connexion à Weaviate pour recherche sémantique

// use serde::{Deserialize, Serialize};
use tracing::info;

pub struct WeaviateMemory {
    url: String,
    client: reqwest::Client,
}

#[derive(Debug, Clone)]
pub struct MemoryHit {
    pub content: String,
    pub source: String,
    #[allow(dead_code)]
    pub timestamp: String,
    pub score: f64,
}

impl WeaviateMemory {
    pub fn new(url: &str, client: reqwest::Client) -> Self {
        Self {
            url: url.trim_end_matches('/').to_string(),
            client,
        }
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<MemoryHit>, String> {
        let gql = format!(
            r#"{{ Get {{ Memory(nearText:{{concepts:["{}"]}} limit:{}) {{ content source timestamp _additional {{ distance }} }} }} }}"#,
            query.replace('"', "\\\""),
            limit
        );

        let resp = self
            .client
            .post(format!("{}/v1/graphql", self.url))
            .json(&serde_json::json!({ "query": gql }))
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("Weaviate error: {}", resp.status()));
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Parse error: {}", e))?;
        let data = json
            .get("data")
            .and_then(|d| d.get("Get"))
            .and_then(|g| g.get("Memory"))
            .and_then(|a| a.as_array())
            .unwrap_or(&vec![])
            .clone();

        let mut hits = Vec::new();
        for item in &data {
            let distance = item
                .get("_additional")
                .and_then(|a| a.get("distance"))
                .and_then(|d| d.as_f64())
                .unwrap_or(1.0);
            hits.push(MemoryHit {
                content: item
                    .get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_string(),
                source: item
                    .get("source")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string(),
                timestamp: item
                    .get("timestamp")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string(),
                score: 1.0 - distance,
            });
        }

        info!("Memory search '{}' → {} hits", query, hits.len());
        Ok(hits)
    }

    pub async fn index(&self, content: &str, source: &str) -> Result<String, String> {
        let resp = self
            .client
            .post(format!("{}/v1/objects", self.url))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "class": "Memory",
                "properties": {
                    "content": content,
                    "source": source,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "session_id": "openclaw-u",
                    "tags": ["openclaw-u"],
                }
            }))
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        if resp.status().is_success() {
            Ok("Indexed".into())
        } else {
            Err(format!("Weaviate error: {}", resp.status()))
        }
    }
}
