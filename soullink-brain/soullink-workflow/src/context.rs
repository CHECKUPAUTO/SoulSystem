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
    /// Ollama base URL for LLM nodes. `None` → echo mode (no model call).
    llm_url: Option<String>,
    /// Model tag used for LLM-node generation.
    llm_model: String,
}

impl WorkflowContext {
    pub fn new(input: &str) -> Self {
        Self {
            state: Arc::new(RwLock::new(HashMap::new())),
            input: input.to_string(),
            mesh_url: "http://127.0.0.1:9020/api/mesh/snapshot".to_string(),
            llm_url: None,
            llm_model: "qwen3:8b".to_string(),
        }
    }

    /// Configure a real Ollama endpoint for LLM/Agent nodes. Without this the
    /// context runs in echo mode (LLM nodes return their assembled prompt).
    pub fn with_llm(mut self, base_url: &str, model: &str) -> Self {
        self.llm_url = Some(base_url.to_string());
        self.llm_model = model.to_string();
        self
    }

    /// Whether a real LLM endpoint is configured.
    pub fn has_llm(&self) -> bool {
        self.llm_url.is_some()
    }

    /// Generate a completion for `prompt` via the configured Ollama endpoint.
    ///
    /// # Errors
    /// Returns an error if no endpoint is configured, the request fails, or the
    /// response cannot be parsed.
    pub async fn generate(&self, prompt: &str) -> Result<String, String> {
        let base = self
            .llm_url
            .as_ref()
            .ok_or_else(|| "no LLM endpoint configured".to_string())?;
        let url = format!("{}/api/generate", base.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": self.llm_model,
            "prompt": prompt,
            "stream": false,
        });
        let resp = reqwest::Client::new()
            .post(&url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(120))
            .send()
            .await
            .map_err(|e| format!("llm request failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("llm status: {}", resp.status()));
        }
        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("llm decode failed: {e}"))?;
        json.get("response")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "llm response missing 'response' field".to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echo_mode_by_default() {
        let ctx = WorkflowContext::new("hi");
        assert!(!ctx.has_llm());
    }

    #[test]
    fn with_llm_enables_endpoint() {
        let ctx = WorkflowContext::new("hi").with_llm("http://localhost:11434", "qwen3:8b");
        assert!(ctx.has_llm());
    }

    #[tokio::test]
    async fn generate_without_endpoint_errors() {
        let ctx = WorkflowContext::new("hi");
        assert!(ctx.generate("prompt").await.is_err());
    }
}
