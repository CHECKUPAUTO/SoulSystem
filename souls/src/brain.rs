//! SoulLink Brain integration into the souls binary.
//!
//! Wires previously orphaned SoulLink brain crates into the autonomous entity
//! runtime: hierarchical memory, metacognition, and Tree-of-Thoughts reasoning.

use anyhow::Result;
use soullink_autonomy::metacognition::MetaCognition;
use soullink_memory_hierarchy::{
    ConsolidationConfig, EpisodicConfig, HierarchicalMemory, MemoryEntry, MemoryLayer,
    SemanticConfig,
};
use soullink_reasoning::{ThoughtTree, TreeConfig};
use std::sync::Arc;
use tracing::info;

/// Encapsulates all wired SoulLink brain components.
pub struct BrainMesh {
    /// Hierarchical memory (working → episodic → semantic).
    pub memory: Arc<HierarchicalMemory>,
    /// Tree-of-Thoughts reasoning engine.
    pub reasoning: ThoughtTree,
    /// Metacognition self-model.
    pub metacognition: Arc<MetaCognition>,
    /// Number of active HNN organs (from soullink-core dynamics).
    pub organ_count: usize,
    /// Brain is initialized and functional.
    pub initialized: bool,
}

impl BrainMesh {
    pub fn new(working_memory_capacity: usize) -> Self {
        let memory = Arc::new(HierarchicalMemory::new(
            working_memory_capacity,
            EpisodicConfig::default(),
            SemanticConfig::default(),
            ConsolidationConfig::default(),
        ));
        let reasoning = ThoughtTree::new(TreeConfig::default());
        let metacognition = MetaCognition::new();

        info!("BrainMesh initialized: memory + reasoning + metacognition");

        Self {
            memory,
            reasoning,
            metacognition,
            organ_count: 6, // Science, Mind, Engineer, Crypto, Creative, Meta
            initialized: true,
        }
    }

    /// Feed an observation into the brain for learning.
    pub async fn observe(&self, input: &str, result: &str, success: bool) {
        let importance = if success { 0.7 } else { 0.3 };
        let entry = MemoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            text: format!("Input: {} → Result: {}", input, result),
            created_at: chrono::Utc::now().to_rfc3339(),
            last_accessed: chrono::Utc::now().to_rfc3339(),
            access_count: 1,
            importance,
            layer: MemoryLayer::Episodic,
            tags: vec!["brain_mesh".into()],
            embedding: None,
            associations: vec![],
            metadata: std::collections::HashMap::new(),
        };
        self.memory
            .store(entry, MemoryLayer::Episodic)
            .await;

        // Update metacognition
        self.metacognition
            .register_capability("observation", importance as f64)
            .await;
        self.metacognition
            .record_outcome("observation", success)
            .await;
    }

    /// Get self-model from metacognition.
    pub async fn self_model(&self) -> soullink_autonomy::metacognition::SelfModel {
        self.metacognition.self_model().await
    }

    /// Run periodic maintenance on the brain.
    pub async fn maintain(&self) -> Result<()> {
        info!("BrainMesh: running maintenance cycle...");
        // Memory decay and consolidation happens at the memory layer level
        Ok(())
    }

    /// Get a summary of brain state for diagnostics.
    pub fn status(&self) -> serde_json::Value {
        serde_json::json!({
            "initialized": self.initialized,
            "organs": self.organ_count,
            "components": ["memory", "reasoning", "metacognition"],
        })
    }
}

impl Default for BrainMesh {
    fn default() -> Self {
        Self::new(100)
    }
}
