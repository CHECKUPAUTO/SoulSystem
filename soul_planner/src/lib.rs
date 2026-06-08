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

    pub fn create_plan(&self, goal: &Goal, available_tools: &[String]) -> Plan {
        let tool_list = if available_tools.is_empty() {
            "No tools available".to_string()
        } else {
            available_tools.join(", ")
        };

        let prompt = format!(
            "You are an autonomous planning system. Decompose this goal into concrete steps.\n\n\
             Goal: {}\n\n\
             Available tools: {}\n\n\
             Respond with a JSON array of steps. Each step should have:\n\
             - \"action\": what to do (be specific)\n\
             - \"tool\": which tool to use (from available tools, or null for general actions)\n\
             - \"args\": arguments for the tool (or null)\n\n\
             Example response:\n\
             [{{\"action\": \"Check disk usage\", \"tool\": \"df\", \"args\": \"-h\"}},\n\
              {{\"action\": \"Analyze results\", \"tool\": null, \"args\": null}}]\n\n\
             Steps (JSON array only, no explanation):",
            goal.description,
            tool_list,
        );

        let steps = match self.call_llm(&prompt) {
            Ok(response) => self.parse_steps_from_llm(&response),
            Err(_) => vec![Step {
                id: Uuid::new_v4().to_string(),
                action: format!("Execute: {}", goal.description),
                tool: None,
                args: None,
                status: StepStatus::Pending,
            }],
        };

        Plan {
            id: Uuid::new_v4().to_string(),
            goal_id: goal.id.clone(),
            steps,
            created_at: Utc::now(),
        }
    }

    fn call_llm(&self, prompt: &str) -> Result<String, String> {
        let client = reqwest::blocking::Client::new();
        let req = serde_json::json!({
            "model": "qwen3:4b",
            "prompt": prompt,
            "stream": false,
            "options": {
                "temperature": 0.3,
                "num_predict": 1024,
            }
        });

        let resp = client
            .post("http://127.0.0.1:11434/api/generate")
            .json(&req)
            .send()
            .map_err(|e| format!("LLM request failed: {}", e))?;

        let body: serde_json::Value = resp
            .json()
            .map_err(|e| format!("LLM response parse failed: {}", e))?;

        body["response"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| "Empty LLM response".to_string())
    }

    fn parse_steps_from_llm(&self, response: &str) -> Vec<Step> {
        let cleaned = response.trim();
        let json_start = cleaned.find('[').unwrap_or(0);
        let json_end = cleaned.rfind(']').map(|i| i + 1).unwrap_or(cleaned.len());
        let json_str = &cleaned[json_start..json_end];

        let parsed: Result<Vec<serde_json::Value>, _> = serde_json::from_str(json_str);
        match parsed {
            Ok(items) => items
                .into_iter()
                .map(|item| Step {
                    id: Uuid::new_v4().to_string(),
                    action: item["action"]
                        .as_str()
                        .unwrap_or("unknown action")
                        .to_string(),
                    tool: item["tool"].as_str().map(|s| s.to_string()),
                    args: item.get("args").cloned(),
                    status: StepStatus::Pending,
                })
                .collect(),
            Err(_) => vec![Step {
                id: Uuid::new_v4().to_string(),
                action: response.lines().next().unwrap_or("parse error").to_string(),
                tool: None,
                args: None,
                status: StepStatus::Pending,
            }],
        }
    }

    pub fn evaluate_plan(&self, plan: &Plan, outcome: &str) -> Evaluation {
        let prompt = format!(
            "Evaluate this plan execution:\n\
             Plan steps: {}\n\
             Outcome: {}\n\n\
             Respond with JSON: {{\"success\": true/false, \"score\": 0.0-1.0, \"feedback\": \"your analysis\"}}",
            plan.steps.len(),
            outcome,
        );

        match self.call_llm(&prompt) {
            Ok(response) => {
                let cleaned = response.trim();
                let json_start = cleaned.find('{').unwrap_or(0);
                let json_end = cleaned.rfind('}').map(|i| i + 1).unwrap_or(cleaned.len());
                let json_str = &cleaned[json_start..json_end];

                if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
                    Evaluation {
                        plan_id: plan.id.clone(),
                        success: val["success"].as_bool().unwrap_or(false),
                        score: val["score"].as_f64().unwrap_or(0.0) as f32,
                        feedback: val["feedback"]
                            .as_str()
                            .unwrap_or("no feedback")
                            .to_string(),
                        timestamp: Utc::now(),
                    }
                } else {
                    self.fallback_evaluate(plan, outcome)
                }
            }
            Err(_) => self.fallback_evaluate(plan, outcome),
        }
    }

    fn fallback_evaluate(&self, plan: &Plan, outcome: &str) -> Evaluation {
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
        let prompt = format!(
            "You are an autonomous agent. Based on this context, decide the best action.\n\n\
             Context: {}\n\n\
             Respond with JSON: {{\"action\": \"what to do\", \"reasoning\": \"why\", \"confidence\": 0.0-1.0, \"alternatives\": [\"other options\"]}}",
            context,
        );

        match self.call_llm(&prompt) {
            Ok(response) => {
                let cleaned = response.trim();
                let json_start = cleaned.find('{').unwrap_or(0);
                let json_end = cleaned.rfind('}').map(|i| i + 1).unwrap_or(cleaned.len());
                let json_str = &cleaned[json_start..json_end];

                if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
                    let alts = val["alternatives"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();

                    Decision {
                        action: val["action"]
                            .as_str()
                            .unwrap_or("continue")
                            .to_string(),
                        reasoning: val["reasoning"]
                            .as_str()
                            .unwrap_or("no reasoning")
                            .to_string(),
                        confidence: val["confidence"].as_f64().unwrap_or(0.5) as f32,
                        alternatives: alts,
                    }
                } else {
                    self.fallback_decide(context)
                }
            }
            Err(_) => self.fallback_decide(context),
        }
    }

    fn fallback_decide(&self, context: &str) -> Decision {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_working_memory() {
        let mut mem = WorkingMemory::new();
        mem.observe("obs1".to_string());
        mem.observe("obs2".to_string());
        assert_eq!(mem.observations.len(), 2);
        let recent = mem.recent_observations(1);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0], "obs2");
    }

    #[test]
    fn test_action_history() {
        let mut hist = ActionHistory::new(5);
        hist.record("action1".to_string(), "result1".to_string(), true);
        hist.record("action2".to_string(), "result2".to_string(), false);
        assert_eq!(hist.actions.len(), 2);
        assert_eq!(hist.success_rate(), 0.5);
    }

    #[test]
    fn test_action_history_max_size() {
        let mut hist = ActionHistory::new(3);
        for i in 0..5 {
            hist.record(format!("a{}", i), format!("r{}", i), true);
        }
        assert_eq!(hist.actions.len(), 3);
    }

    #[test]
    fn test_action_history_recent() {
        let mut hist = ActionHistory::new(100);
        for i in 0..5 {
            hist.record(format!("a{}", i), format!("r{}", i), true);
        }
        let recent = hist.recent(3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].action, "a2");
    }

    #[test]
    fn test_action_history_empty() {
        let hist = ActionHistory::new(100);
        assert_eq!(hist.success_rate(), 1.0);
    }

    #[test]
    fn test_goal_creation() {
        let goal = Goal {
            id: "test-id".to_string(),
            description: "Test goal".to_string(),
            priority: 5,
            created_at: Utc::now(),
            status: GoalStatus::Active,
        };
        assert_eq!(goal.description, "Test goal");
        assert_eq!(goal.priority, 5);
    }

    #[test]
    fn test_plan_creation() {
        let goal = Goal {
            id: "g1".to_string(),
            description: "test".to_string(),
            priority: 5,
            created_at: Utc::now(),
            status: GoalStatus::Active,
        };
        let loop_ = CognitiveLoop::new();
        let plan = loop_.create_plan(&goal, &[]);
        assert_eq!(plan.goal_id, "g1");
        assert!(!plan.steps.is_empty());
    }

    #[test]
    fn test_plan_with_tools() {
        let goal = Goal {
            id: "g1".to_string(),
            description: "check disk".to_string(),
            priority: 5,
            created_at: Utc::now(),
            status: GoalStatus::Active,
        };
        let loop_ = CognitiveLoop::new();
        let plan = loop_.create_plan(&goal, &["df".to_string(), "ls".to_string()]);
        assert!(!plan.steps.is_empty());
    }

    #[test]
    fn test_decision_creation() {
        let decision = Decision {
            action: "test".to_string(),
            reasoning: "because".to_string(),
            confidence: 0.9,
            alternatives: vec!["alt1".to_string()],
        };
        assert_eq!(decision.action, "test");
        assert_eq!(decision.confidence, 0.9);
    }
}
