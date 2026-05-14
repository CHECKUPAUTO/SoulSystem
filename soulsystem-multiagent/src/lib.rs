pub struct SpecializedAgent {
    pub name: String,
    pub model: String,
    pub tools: Vec<String>,
    pub skills: Vec<String>,
    pub is_available: bool,
}

pub struct AgentTask {
    pub id: String,
    pub task_type: String,
    pub description: String,
    pub requires_gpu: bool,
}

pub struct AgentResult {
    pub task_id: String,
    pub agent_name: String,
    pub success: bool,
    pub output: String,
}

pub struct TaskRouter {
    default_agent: String,
}

impl TaskRouter {
    pub fn new(default: &str) -> Self { Self { default_agent: default.to_string() } }
    pub fn route(&self, task: &AgentTask) -> &str {
        match task.task_type.as_str() {
            "code" => "coder",
            "research" => "researcher",
            "review" => "reviewer",
            _ => &self.default_agent,
        }
    }
}

pub struct AgentOrchestrator {
    agents: Vec<SpecializedAgent>,
}

impl AgentOrchestrator {
    pub fn new() -> Self { Self { agents: Vec::new() } }
    pub fn register_agent(&mut self, agent: SpecializedAgent) { self.agents.push(agent); }
    pub fn dispatch(&self, task: &AgentTask) -> Option<&SpecializedAgent> {
        self.agents.iter().find(|a| a.name == TaskRouter::new("coder").route(task) && a.is_available)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_router_default() { assert_eq!(TaskRouter::new("default").route(&AgentTask { id: "1".into(), task_type: "other".into(), description: "".into(), requires_gpu: false }), "default"); }
    #[test] fn test_router_code() { assert_eq!(TaskRouter::new("default").route(&AgentTask { id: "1".into(), task_type: "code".into(), description: "".into(), requires_gpu: false }), "coder"); }
}
