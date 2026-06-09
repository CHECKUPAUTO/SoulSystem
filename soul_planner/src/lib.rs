//! # soul_planner — Boucle Cognitive
//!
//! Observe → Planifie → Agit → Évalue → Décide.
//!
//! ## Exemple
//! ```ignore
//! use soul_planner::*;
//! let mut planner = CognitiveLoop::new();
//! let goal = Goal {
//!     id: uuid::Uuid::new_v4().to_string(),
//!     description: "Analyser les logs".into(),
//!     priority: 5,
//!     created_at: chrono::Utc::now(),
//!     status: GoalStatus::Active,
//! };
//! planner.create_plan(&goal, &["ls", "grep"]);
//! ```

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Types de buts ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub description: String,
    pub priority: u8, // 1-10
    pub created_at: chrono::DateTime<Utc>,
    #[serde(default = "active_default")]
    pub status: GoalStatus,
}

fn active_default() -> GoalStatus {
    GoalStatus::Active
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GoalStatus {
    Active,
    InProgress,
    Completed,
    Failed,
}

// ── Types de plan ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub goal_id: String,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub command: String,
    pub description: String,
    #[serde(default = "default_order")]
    pub order: usize,
}

fn default_order() -> usize {
    0
}

// ── Évaluation ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evaluation {
    pub score: f32, // 0.0 - 1.0
    pub feedback: String,
}

// ── Décision ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub action: String,
    pub reasoning: String,
    pub confidence: f32, // 0.0 - 1.0
}

// ── Mémoire de travail (buffer circulaire) ───────────────────

#[derive(Debug, Clone)]
pub struct WorkingMemory {
    buffer: Vec<String>,
    max_size: usize,
}

impl WorkingMemory {
    pub fn new(max_size: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(max_size),
            max_size,
        }
    }

    pub fn observe(&mut self, observation: String) {
        self.buffer.push(observation);
        if self.buffer.len() > self.max_size {
            self.buffer.remove(0);
        }
    }

    pub fn recent_observations(&self, n: usize) -> Vec<String> {
        let start = self.buffer.len().saturating_sub(n);
        self.buffer[start..].to_vec()
    }
}

// ── Historique d'actions ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRecord {
    pub command: String,
    pub result: String,
    pub success: bool,
    pub timestamp: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ActionHistory {
    records: Vec<ActionRecord>,
    max_size: usize,
}

impl ActionHistory {
    pub fn new(max_size: usize) -> Self {
        Self {
            records: Vec::with_capacity(max_size),
            max_size,
        }
    }

    pub fn record(&mut self, command: String, result: String, success: bool) {
        self.records.push(ActionRecord {
            command,
            result,
            success,
            timestamp: Utc::now(),
        });
        if self.records.len() > self.max_size {
            self.records.remove(0);
        }
    }

    pub fn success_rate(&self) -> f32 {
        if self.records.is_empty() {
            return 0.0;
        }
        let success_count = self.records.iter().filter(|r| r.success).count();
        success_count as f32 / self.records.len() as f32
    }

    pub fn recent(&self, n: usize) -> Vec<&ActionRecord> {
        let start = self.records.len().saturating_sub(n);
        self.records[start..].iter().collect()
    }
}

// ── Boucle cognitive ────────────────────────────────────────

pub struct CognitiveLoop {
    pub memory: WorkingMemory,
    pub history: ActionHistory,
}

impl CognitiveLoop {
    pub fn new() -> Self {
        Self {
            memory: WorkingMemory::new(100),
            history: ActionHistory::new(200),
        }
    }

    pub fn create_plan(&self, goal: &Goal, step_commands: &[&str]) -> Plan {
        Plan {
            id: Uuid::new_v4().to_string(),
            goal_id: goal.id.clone(),
            steps: step_commands
                .iter()
                .enumerate()
                .map(|(i, cmd)| Step {
                    command: cmd.to_string(),
                    description: format!("Étape {}: {}", i + 1, goal.description),
                    order: i,
                })
                .collect(),
        }
    }

    pub fn evaluate_plan(&self, _plan: &Plan, outcome: &str) -> Evaluation {
        // Simplifié — un vrai impl ferait une analyse LLM ou heuristique
        let score = if outcome.contains("success") || outcome.contains("ok") {
            0.95
        } else if outcome.contains("error") {
            0.1
        } else {
            0.5
        };

        Evaluation {
            score,
            feedback: outcome.to_string(),
        }
    }

    pub fn decide(&self, context: &str) -> Decision {
        Decision {
            action: "continue".into(),
            reasoning: format!("Décision basée sur le contexte: {}", context),
            confidence: 0.8,
        }
    }
}

impl Default for CognitiveLoop {
    fn default() -> Self {
        Self::new()
    }
}
