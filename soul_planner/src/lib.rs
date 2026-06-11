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
    pub key_info: String,
    pub related_sop: Option<String>,
    pub last_updated: DateTime<Utc>,
}

impl Default for WorkingMemory {
    fn default() -> Self {
        Self {
            observations: Vec::new(),
            context: serde_json::json!({}),
            key_info: String::new(),
            related_sop: None,
            last_updated: Utc::now(),
        }
    }
}

impl WorkingMemory {
    pub fn new() -> Self {
        Self::default()
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

    pub fn set_key_info(&mut self, info: &str) {
        self.key_info = info.to_string();
        self.last_updated = Utc::now();
    }

    /// Build the anchor prompt injected into every turn
    pub fn to_prompt_section(&self) -> String {
        let mut parts = Vec::new();

        if !self.key_info.is_empty() {
            parts.push(format!("<key_info>\n{}\n</key_info>", self.key_info));
        }

        if let Some(sop) = &self.related_sop {
            parts.push(format!("<related_sop>\n{}\n</related_sop>", sop));
        }

        let recent = self.recent_observations(5);
        if !recent.is_empty() {
            let obs_text = recent.join("\n  - ");
            parts.push(format!(
                "<recent_observations>\n  - {}\n</recent_observations>",
                obs_text
            ));
        }

        if parts.is_empty() {
            String::new()
        } else {
            format!("\n## Working Context\n{}\n", parts.join("\n"))
        }
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

    pub fn recent_summaries(&self, n: usize) -> Vec<String> {
        self.recent(n)
            .iter()
            .map(|a| {
                let status = if a.success { "OK" } else { "FAIL" };
                format!("[{}] {} → {}", status, a.action, truncate(&a.result, 100))
            })
            .collect()
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

// ── Cognitive Loop (LLM-powered) ─────────────────────────────────────

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

    /// Create a plan by asking the LLM to decompose the goal into steps
    pub async fn create_plan_llm(
        &self,
        goal: &Goal,
        available_tools: &[String],
        llm: &soul_llm::OllamaClient,
    ) -> Plan {
        let tools_list = available_tools.join(", ");
        let history = self.history.recent_summaries(5);
        let history_section = if history.is_empty() {
            String::new()
        } else {
            format!("\nRecent actions:\n{}", history.join("\n"))
        };

        let prompt = format!(
            r#"You are a planning engine. Break this goal into concrete, actionable steps.

GOAL: {goal}
AVAILABLE TOOLS: {tools}{history}

Return a JSON array of steps. Each step is: {{"action": "description", "tool": "tool_name_or_null", "args": {{}}}}
Only return the JSON array, no explanation.
Example: [{{"action": "list files in /tmp", "tool": "ls", "args": {{"path": "/tmp"}}}}]"#,
            goal = goal.description,
            tools = tools_list,
            history = history_section,
        );

        match llm.generate(&prompt).await {
            Ok(resp) => {
                let steps = parse_plan_steps(&resp.response);
                Plan {
                    id: Uuid::new_v4().to_string(),
                    goal_id: goal.id.clone(),
                    steps,
                    created_at: Utc::now(),
                }
            }
            Err(e) => {
                tracing::warn!("LLM planning failed, falling back to single step: {}", e);
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
        }
    }

    /// Fallback: create plan without LLM
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

    /// Ask the LLM to decide what to do next
    pub async fn decide_llm(&self, context: &str, llm: &soul_llm::OllamaClient) -> Decision {
        let history = self.history.recent_summaries(5);
        let prompt = format!(
            r#"You are a decision engine for an autonomous agent.

CURRENT CONTEXT: {context}

RECENT ACTIONS:
{history}

Decide what to do next. Return JSON:
{{"action": "what to do", "reasoning": "why", "confidence": 0.0-1.0, "alternatives": ["alt1", "alt2"]}}
Only return the JSON, no explanation."#,
            context = context,
            history = history.join("\n"),
        );

        match llm.generate(&prompt).await {
            Ok(resp) => parse_decision(&resp.response),
            Err(e) => {
                tracing::warn!("LLM decision failed: {}", e);
                Decision {
                    action: "continue".to_string(),
                    reasoning: format!("LLM unavailable: {}", e),
                    confidence: 0.5,
                    alternatives: vec!["retry".to_string(), "abort".to_string()],
                }
            }
        }
    }

    /// Fallback: decide without LLM
    pub fn decide(&self, context: &str) -> Decision {
        Decision {
            action: "continue".to_string(),
            reasoning: format!("Based on context: {}", context),
            confidence: 0.8,
            alternatives: vec!["retry".to_string(), "abort".to_string()],
        }
    }

    pub fn evaluate_plan(&self, plan: &Plan, outcome: &str) -> Evaluation {
        let success = outcome.to_lowercase().contains("success")
            || outcome.to_lowercase().contains("done")
            || outcome.to_lowercase().contains("completed");
        Evaluation {
            plan_id: plan.id.clone(),
            success,
            score: if success { 1.0 } else { 0.0 },
            feedback: outcome.to_string(),
            timestamp: Utc::now(),
        }
    }
}

impl Default for CognitiveLoop {
    fn default() -> Self {
        Self::new()
    }
}

// ── JSON Parsing Helpers ──────────────────────────────────────────────

fn parse_plan_steps(response: &str) -> Vec<Step> {
    // Try to extract JSON array from response
    let json_str = extract_json_array(response);

    match serde_json::from_str::<Vec<serde_json::Value>>(&json_str) {
        Ok(values) => values
            .into_iter()
            .map(|v| Step {
                id: Uuid::new_v4().to_string(),
                action: v
                    .get("action")
                    .and_then(|a| a.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                tool: v
                    .get("tool")
                    .and_then(|t| t.as_str())
                    .filter(|t| *t != "null" && !t.is_empty())
                    .map(|t| t.to_string()),
                args: v.get("args").cloned(),
                status: StepStatus::Pending,
            })
            .collect(),
        Err(e) => {
            tracing::warn!("Failed to parse plan JSON: {}", e);
            vec![Step {
                id: Uuid::new_v4().to_string(),
                action: response.to_string(),
                tool: None,
                args: None,
                status: StepStatus::Pending,
            }]
        }
    }
}

fn parse_decision(response: &str) -> Decision {
    let json_str = extract_json_object(response);

    match serde_json::from_str::<serde_json::Value>(&json_str) {
        Ok(v) => Decision {
            action: v
                .get("action")
                .and_then(|a| a.as_str())
                .unwrap_or("continue")
                .to_string(),
            reasoning: v
                .get("reasoning")
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_string(),
            confidence: v.get("confidence").and_then(|c| c.as_f64()).unwrap_or(0.5) as f32,
            alternatives: v
                .get("alternatives")
                .and_then(|a| a.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
        },
        Err(e) => {
            tracing::warn!("Failed to parse decision JSON: {}", e);
            Decision {
                action: "continue".to_string(),
                reasoning: response.to_string(),
                confidence: 0.5,
                alternatives: vec![],
            }
        }
    }
}

fn extract_json_array(text: &str) -> String {
    // Find [ ... ] in the text
    if let Some(start) = text.find('[') {
        if let Some(end) = text.rfind(']') {
            return text[start..=end].to_string();
        }
    }
    text.to_string()
}

fn extract_json_object(text: &str) -> String {
    // Find { ... } in the text
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            return text[start..=end].to_string();
        }
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_working_memory() {
        let mut wm = WorkingMemory::new();
        assert!(wm.observations.is_empty());

        wm.observe("test observation".to_string());
        assert_eq!(wm.observations.len(), 1);
        assert_eq!(wm.recent_observations(10)[0], "test observation");
    }

    #[test]
    fn test_working_memory_key_info() {
        let mut wm = WorkingMemory::new();
        wm.set_key_info("important context");
        assert_eq!(wm.key_info, "important context");
    }

    #[test]
    fn test_working_memory_to_prompt() {
        let mut wm = WorkingMemory::new();
        wm.set_key_info("task: explore server");
        wm.observe("found 10 files".to_string());
        let prompt = wm.to_prompt_section();
        assert!(prompt.contains("key_info"));
        assert!(prompt.contains("explore server"));
        assert!(prompt.contains("found 10 files"));
    }

    #[test]
    fn test_action_history() {
        let mut h = ActionHistory::new(3);
        h.record("cmd1".to_string(), "out1".to_string(), true);
        h.record("cmd2".to_string(), "out2".to_string(), false);
        h.record("cmd3".to_string(), "out3".to_string(), true);

        assert_eq!(h.actions.len(), 3);
        assert!((h.success_rate() - 0.667).abs() < 0.01);

        // Should evict oldest
        h.record("cmd4".to_string(), "out4".to_string(), true);
        assert_eq!(h.actions.len(), 3);
        assert_eq!(h.actions[0].action, "cmd2");
    }

    #[test]
    fn test_action_history_summaries() {
        let mut h = ActionHistory::new(10);
        h.record("ls /tmp".to_string(), "file1\nfile2".to_string(), true);
        h.record("rm bad".to_string(), "permission denied".to_string(), false);

        let summaries = h.recent_summaries(5);
        assert_eq!(summaries.len(), 2);
        assert!(summaries[0].contains("OK"));
        assert!(summaries[1].contains("FAIL"));
    }

    #[test]
    fn test_cognitive_loop_fallback() {
        let loop_ = CognitiveLoop::new();
        let goal = Goal {
            id: "g1".to_string(),
            description: "test goal".to_string(),
            priority: 5,
            created_at: Utc::now(),
            status: GoalStatus::Active,
        };
        let plan = loop_.create_plan(&goal, &[]);
        assert_eq!(plan.steps.len(), 1);
        assert!(plan.steps[0].action.contains("test goal"));

        let decision = loop_.decide("context");
        assert_eq!(decision.action, "continue");
    }

    #[test]
    fn test_evaluate_plan() {
        let loop_ = CognitiveLoop::new();
        let plan = Plan {
            id: "p1".to_string(),
            goal_id: "g1".to_string(),
            steps: vec![],
            created_at: Utc::now(),
        };
        let eval = loop_.evaluate_plan(&plan, "Task completed successfully");
        assert!(eval.success);
        assert_eq!(eval.score, 1.0);

        let eval2 = loop_.evaluate_plan(&plan, "something went wrong");
        assert!(!eval2.success);
    }

    #[test]
    fn test_parse_plan_steps() {
        let response = r#"[{"action": "list files", "tool": "ls", "args": {"path": "/tmp"}}]"#;
        let steps = parse_plan_steps(response);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].action, "list files");
        assert_eq!(steps[0].tool.as_deref(), Some("ls"));
    }

    #[test]
    fn test_parse_plan_steps_no_json() {
        let response = "Just do the thing";
        let steps = parse_plan_steps(response);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].action, "Just do the thing");
    }

    #[test]
    fn test_parse_decision() {
        let response = r#"{"action": "run tests", "reasoning": "to verify", "confidence": 0.9, "alternatives": ["skip"]}"#;
        let d = parse_decision(response);
        assert_eq!(d.action, "run tests");
        assert_eq!(d.confidence, 0.9);
        assert_eq!(d.alternatives, vec!["skip"]);
    }

    #[test]
    fn test_extract_json() {
        let text = "Here is the result: [{\"a\": 1}] done.";
        let arr = extract_json_array(text);
        assert!(arr.starts_with('['));
        assert!(arr.ends_with(']'));

        let text2 = "Result: {\"key\": \"value\"} end.";
        let obj = extract_json_object(text2);
        assert!(obj.starts_with('{'));
        assert!(obj.ends_with('}'));
    }
}
