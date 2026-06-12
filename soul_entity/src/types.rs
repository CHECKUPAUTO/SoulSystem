use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use soul_llm::LlmConfig;
use soul_openclaw::AgentLoopConfig;
use soul_sandbox::{SandboxPolicy, SandboxVerdict};
use std::path::PathBuf;
use std::time::Duration;

// ── Configuration de l'entité ─────────────────────────────────

#[derive(Debug, Clone)]
pub struct EntityConfig {
    pub name: String,
    pub llm: LlmConfig,
    pub sandbox_policy: SandboxPolicy,
    pub loop_config: AgentLoopConfig,
    pub autonomous_tick: Duration,
    pub memory_path: Option<PathBuf>,
    pub max_goal_history: usize,
    pub event_store_path: Option<PathBuf>,
}

impl Default for EntityConfig {
    fn default() -> Self {
        Self {
            name: "soul".into(),
            llm: LlmConfig::default(),
            sandbox_policy: SandboxPolicy::default(),
            loop_config: AgentLoopConfig::default(),
            autonomous_tick: Duration::from_millis(750),
            memory_path: None,
            event_store_path: None,
            max_goal_history: 50,
        }
    }
}

// ── But persistant (avec statut) ──────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentGoal {
    pub id: String,
    pub description: String,
    pub priority: u8,
    pub importance: f32,          // 0.0-1.0 importance weight
    pub deadline: Option<DateTime<Utc>>,  // Optional deadline
    pub depends_on: Vec<String>,  // Goal IDs this depends on
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub plan: Option<PersistentPlan>,
    pub last_evaluation: Option<soul_planner::Evaluation>,
    pub last_decision: Option<soul_planner::Decision>,
    pub llm_prompt_tokens: usize,
    pub llm_completion_tokens: usize,
    pub llm_total_tokens: usize,
    pub llm_request_count: usize,
}

impl Default for PersistentGoal {
    fn default() -> Self {
        Self {
            id: String::new(),
            description: String::new(),
            priority: 0,
            importance: 0.5,
            deadline: None,
            depends_on: Vec::new(),
            status: String::new(),
            created_at: Utc::now(),
            plan: None,
            last_evaluation: None,
            last_decision: None,
            llm_prompt_tokens: 0,
            llm_completion_tokens: 0,
            llm_total_tokens: 0,
            llm_request_count: 0,
        }
    }
}

const PRIORITY_MULTIPLIER: f32 = 10.0;
const IMPORTANCE_MULTIPLIER: f32 = 50.0;
const URGENCY_WEIGHT: f32 = 100.0;
const MIN_HOURS_URGENCY: f32 = 0.1;
const OVERDUE_BONUS: f32 = 1000.0;

impl PersistentGoal {
    /// Calcule un score de priorité composite pour l'ordonnancement
    pub fn priority_score(&self) -> f32 {
        let mut score = self.priority as f32 * PRIORITY_MULTIPLIER;
        score += self.importance * IMPORTANCE_MULTIPLIER;

        // Urgence basée sur deadline
        if let Some(deadline) = self.deadline {
            let now = Utc::now();
            let hours_until = deadline.signed_duration_since(now).num_seconds() as f32 / 3600.0;
            if hours_until > 0.0 {
                score += URGENCY_WEIGHT / hours_until.max(MIN_HOURS_URGENCY);
            } else {
                score += OVERDUE_BONUS;
            }
        }

        score
    }

    /// Vérifie si les dépendances sont satisfaites
    pub fn dependencies_satisfied(&self, completed_goals: &std::collections::HashSet<String>) -> bool {
        self.depends_on.iter().all(|dep| completed_goals.contains(dep))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentPlan {
    pub id: String,
    pub steps: Vec<String>,
}

// ── Artefact de code auto-généré ──────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeArtifact {
    pub id: String,
    pub language: String,
    pub source: String,
    pub verdict: Option<SandboxVerdict>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntityStats {
    pub cycles_run: u64,
    pub cycles_succeeded: u64,
    pub cycles_failed: u64,
    pub goals_completed: u64,
    pub goals_failed: u64,
    pub tools_executed: u64,
    pub code_artifacts_generated: u64,
    pub last_cycle_at: Option<DateTime<Utc>>,
}
