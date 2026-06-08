use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub description: String,
    pub priority: u8,
    pub created_at: DateTime<Utc>,
    pub status: GoalStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GoalStatus {
    Active,
    Completed,
    Failed,
    Deferred,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub id: String,
    pub action: String,
    pub tool: Option<String>,
    pub args: Option<serde_json::Value>,
    pub status: StepStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StepStatus {
    Pending,
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub goal_id: String,
    pub steps: Vec<Step>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evaluation {
    pub plan_id: String,
    pub success: bool,
    pub score: f32,
    pub feedback: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub action: String,
    pub reasoning: String,
    pub confidence: f32,
    pub alternatives: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRecord {
    pub id: String,
    pub action: String,
    pub result: String,
    pub success: bool,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkingMemory {
    pub observations: Vec<String>,
    pub context: serde_json::Value,
    pub last_updated: DateTime<Utc>,
}

impl WorkingMemory {
    pub fn new() -> Self {
        Self {
            observations: Vec::new(),
            context: serde_json::json!({}),
            last_updated: Utc::now(),
        }
    }

    pub fn observe(&mut self, observation: String) {
        self.observations.push(observation);
        self.last_updated = Utc::now();
    }

    pub fn recent_observations(&self, n: usize) -> &[String] {
        let len = self.observations.len();
        let start = len.saturating_sub(n);
        &self.observations[start..]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionHistory {
    pub actions: Vec<ActionRecord>,
    max_size: usize,
}

impl ActionHistory {
    pub fn new(max_size: usize) -> Self {
        Self {
            actions: Vec::new(),
            max_size,
        }
    }

    pub fn record(&mut self, action: String, result: String, success: bool) {
        let record = ActionRecord {
            id: Uuid::new_v4().to_string(),
            action,
            result,
            success,
            timestamp: Utc::now(),
        };
        self.actions.push(record);
        if self.actions.len() > self.max_size {
            self.actions.remove(0);
        }
    }

    pub fn recent(&self, n: usize) -> &[ActionRecord] {
        let len = self.actions.len();
        let start = len.saturating_sub(n);
        &self.actions[start..]
    }

    pub fn success_rate(&self) -> f32 {
        if self.actions.is_empty() {
            return 1.0;
        }
        let successes = self.actions.iter().filter(|a| a.success).count() as f32;
        successes / self.actions.len() as f32
    }
}

pub struct CognitiveLoop {
    pub memory: WorkingMemory,
    pub history: ActionHistory,
}

impl CognitiveLoop {
    pub fn new() -> Self {
        Self {
            memory: WorkingMemory::new(),
            history: ActionHistory::new(100),
        }
    }

    pub fn create_plan(&self, goal: &Goal, _available_tools: &[String]) -> Plan {
        Plan {
            id: Uuid::new_v4().to_string(),
            goal_id: goal.id.clone(),
            steps: vec![Step {
                id: Uuid::new_v4().to_string(),
                action: format!("Execute: {}", goal.description),
                tool: None,
                args: None,
                status: StepStatus::Pending,
            }],
            created_at: Utc::now(),
        }
    }

    pub fn evaluate_plan(&self, plan: &Plan, outcome: &str) -> Evaluation {
        let success = outcome.to_lowercase().contains("success")
            || outcome.to_lowercase().contains("done");
        Evaluation {
            plan_id: plan.id.clone(),
            success,
            score: if success { 1.0 } else { 0.0 },
            feedback: outcome.to_string(),
            timestamp: Utc::now(),
        }
    }

    pub fn decide(&self, context: &str) -> Decision {
        Decision {
            action: "continue".to_string(),
            reasoning: format!("Based on context: {}", context),
            confidence: 0.8,
            alternatives: vec!["retry".to_string(), "abort".to_string()],
        }
    }
}

impl Default for CognitiveLoop {
    fn default() -> Self {
        Self::new()
    }
}
