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
    router: TaskRouter,
}

impl AgentOrchestrator {
    pub fn new(default_agent: &str) -> Self {
        Self {
            agents: Vec::new(),
            router: TaskRouter::new(default_agent),
        }
    }

    pub fn register_agent(&mut self, agent: SpecializedAgent) {
        self.agents.push(agent);
    }

    pub fn dispatch(&self, task: &AgentTask) -> Option<&SpecializedAgent> {
        let agent_name = self.router.route(task);
        self.agents.iter().find(|a| a.name == agent_name && a.is_available)
    }

    pub fn process_task(&self, task: &AgentTask) -> AgentResult {
        match self.dispatch(task) {
            Some(agent) => {
                tracing::info!("Agent {} is processing task {}", agent.name, task.id);
                AgentResult {
                    task_id: task.id.clone(),
                    agent_name: agent.name.clone(),
                    success: true,
                    output: format!("Result from {}: Processed '{}'", agent.name, task.description),
                }
            }
            None => {
                tracing::warn!("No available agent found for task {}", task.id);
                AgentResult {
                    task_id: task.id.clone(),
                    agent_name: "none".into(),
                    success: false,
                    output: "Error: No specialized agent available for this task type".into(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_default() {
        assert_eq!(TaskRouter::new("default").route(&AgentTask { id: "1".into(), task_type: "other".into(), description: "".into(), requires_gpu: false }), "default");
    }

    #[test]
    fn test_router_code() {
        assert_eq!(TaskRouter::new("default").route(&AgentTask { id: "1".into(), task_type: "code".into(), description: "".into(), requires_gpu: false }), "coder");
    }

    #[test]
    fn test_orchestrator_dispatch() {
        let mut orch = AgentOrchestrator::new("default");
        orch.register_agent(SpecializedAgent {
            name: "coder".into(),
            model: "gpt-4".into(),
            tools: vec![],
            skills: vec![],
            is_available: true,
        });

        let task = AgentTask {
            id: "t1".into(),
            task_type: "code".into(),
            description: "write a fib function".into(),
            requires_gpu: false,
        };

        let agent = orch.dispatch(&task).unwrap();
        assert_eq!(agent.name, "coder");

        let result = orch.process_task(&task);
        assert!(result.success);
        assert!(result.output.contains("Processed 'write a fib function'"));
    }

    #[test]
    fn test_orchestrator_no_agent() {
        let orch = AgentOrchestrator::new("default");
        let task = AgentTask {
            id: "t2".into(),
            task_type: "research".into(),
            description: "find papers on HNN".into(),
            requires_gpu: false,
        };

        let result = orch.process_task(&task);
        assert!(!result.success);
        assert_eq!(result.agent_name, "none");
    }
}
