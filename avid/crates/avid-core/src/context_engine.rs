use crate::memory::MemoryStore;
use std::sync::Arc;
use tracing::info;

/// The ContextEngine manages repository-wide context, indexing, and retrieval.
pub struct ContextEngine {
    memory: Arc<MemoryStore>,
}

impl ContextEngine {
    #[must_use]
    pub fn new(memory: Arc<MemoryStore>) -> Self {
        Self { memory }
    }

    /// Index a project directory for future retrieval.
    pub async fn index_project(&self, path: &str) -> anyhow::Result<()> {
        info!(path, "indexing project context");
        // Implementation for semantic indexing would go here
        // For now, we use the MemoryStore to track indexed state
        self.memory
            .store_trace(
                "system",
                "indexing",
                &serde_json::json!({"path": path, "status": "completed"}),
            )
            .await?;
        Ok(())
    }

    /// Retrieve relevant context for a given query using RAG.
    pub async fn get_context(&self, query: &str) -> anyhow::Result<String> {
        info!(query, "retrieving context");
        let recall = self.memory.recall(query, 5).await?;
        let context = recall
            .iter()
            .map(|r| format!("Context [{}]: {}", r.task_text, r.payload_summary))
            .collect::<Vec<_>>()
            .join("\n---\n");

        Ok(context)
    }
}
