//! ThoughtNode — a single step in the Tree of Thoughts.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Status of a thought node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeStatus {
    /// Just created, not yet evaluated.
    Pending,
    /// Evaluated and accepted (high semantic score).
    Accepted,
    /// Evaluated and pruned (low semantic score).
    Pruned,
    /// Currently being explored.
    Exploring,
}

/// A single step in the Tree of Thoughts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThoughtNode {
    /// Unique node ID.
    pub id: usize,
    /// The reasoning text for this step.
    pub content: String,
    /// Parent node ID (None for root).
    pub parent_id: Option<usize>,
    /// Child node IDs.
    pub children: Vec<usize>,
    /// Depth in the tree (0 = root).
    pub depth: usize,
    /// Semantic similarity score from soullink-eval (0.0–1.0).
    pub score: f32,
    /// Loss value (1 - score).
    pub loss: f32,
    /// Node status.
    pub status: NodeStatus,
    /// Timestamp of creation.
    pub created_at: DateTime<Utc>,
}

impl ThoughtNode {
    pub fn new(id: usize, content: impl Into<String>, parent_id: Option<usize>, depth: usize) -> Self {
        Self {
            id,
            content: content.into(),
            parent_id,
            children: Vec::new(),
            depth,
            score: 0.0,
            loss: 1.0,
            status: NodeStatus::Pending,
            created_at: Utc::now(),
        }
    }

    /// Create a root node.
    pub fn root(id: usize, content: impl Into<String>) -> Self {
        Self::new(id, content, None, 0)
    }

    /// Mark this node as accepted with the given score.
    pub fn accept(&mut self, score: f32) {
        self.score = score;
        self.loss = 1.0 - score;
        self.status = NodeStatus::Accepted;
    }

    /// Mark this node as pruned.
    pub fn prune(&mut self) {
        self.status = NodeStatus::Pruned;
    }

    /// Mark as currently being explored.
    pub fn exploring(&mut self) {
        self.status = NodeStatus::Exploring;
    }

    /// Is this a leaf node (no children)?
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }
}