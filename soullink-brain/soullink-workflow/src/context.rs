//! Shared workflow context: accessible by all nodes during execution.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared workflow state accessible by all nodes.
#[derive(Clone)]
pub struct WorkflowContext {
    state: Arc<RwLock<HashMap<String, serde_json::Value>>>,
    input: String,
    mesh_url: String,
}

impl WorkflowContext {
    pub fn new(input: &str) -> Self {
        Self {
            state: Arc::new(RwLock::new(HashMap::new())),
            input: input.to_string(),
            mesh_url: "http://127.0.0.1:9020/api/mesh/snapshot".to_string(),
        }
    }

    /// Set a node's output in shared state.
    pub async fn set(&self, key: &str, value: &str) {
        self.state.write().await.insert(
            key.to_string(),
            serde_json::Value::String(value.to_string()),
        );
    }

    /// Get a node's output from shared state.
    pub async fn get(&self, key: &str) -> Option<String> {
        self.state
            .read()
            .await
            .get(key)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
    }

    /// Get all context as a formatted string for AI prompt injection.
    pub async fn context_for_node(&self, _node_id: &str, deps: &[String]) -> String {
        let state = self.state.read().await;
        let mut parts = vec![format!("=== Input ===\n{}", self.input)];
        for dep in deps {
            if let Some(val) = state.get(dep) {
                parts.push(format!("=== Output from {dep} ===\n{val}"));
            }
        }
        parts.join("\n\n")
    }

    /// Fetch HNN mesh snapshot from the local API.
    pub async fn fetch_mesh_snapshot(&self) -> Result<String, String> {
        let client = reqwest::Client::new();
        let resp = client
            .get(&self.mesh_url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| format!("mesh request failed: {e}"))?;

        if resp.status().is_success() {
            resp.text()
                .await
                .map_err(|e| format!("mesh read failed: {e}"))
        } else {
            Err(format!("mesh status: {}", resp.status()))
        }
    }
}
