use soul_llm::{LlmConfig, OllamaClient};
use soul_planner::{CognitiveLoop, Goal, Plan};
use soul_tools::{discover_system_tools, execute_shell, Tool, ToolRegistry};

pub struct AutonomousEntity {
    pub llm: OllamaClient,
    pub planner: CognitiveLoop,
    pub registry: ToolRegistry,
    pub name: String,
}

impl AutonomousEntity {
    pub fn new(config: LlmConfig, name: &str) -> Self {
        let tools = discover_system_tools();
        let mut registry = ToolRegistry::new();
        for tool in tools {
            registry.register(tool);
        }
        Self {
            llm: OllamaClient::new(config),
            planner: CognitiveLoop::new(),
            registry,
            name: name.to_string(),
        }
    }

    pub async fn is_alive(&self) -> bool {
        self.llm.is_alive().await
    }

    pub fn create_goal(&self, description: &str) -> Goal {
        Goal {
            id: uuid::Uuid::new_v4().to_string(),
            description: description.to_string(),
            priority: 5,
            created_at: chrono::Utc::now(),
            status: soul_planner::GoalStatus::Active,
        }
    }

    pub fn plan(&self, goal: &Goal) -> Plan {
        let tool_names: Vec<String> = self.registry.list().iter().map(|t| t.name.clone()).collect();
        self.planner.create_plan(goal, &tool_names)
    }

    pub async fn ask(&self, prompt: &str) -> Result<String, soul_llm::LlmError> {
        let resp = self.llm.generate(prompt).await?;
        Ok(resp.response)
    }

    pub fn execute_plan(&mut self, plan: &Plan) -> Result<String, String> {
        let mut results = Vec::new();
        for step in &plan.steps {
            let result = match &step.tool {
                Some(tool_name) => {
                    if let Some(tool) = self.registry.get(tool_name) {
                        let args = step.args.as_ref().map(|a| a.to_string()).unwrap_or_default();
                        soul_tools::execute_tool(tool, &args)?
                    } else {
                        format!("Tool '{}' not found", tool_name)
                    }
                }
                None => {
                    execute_shell(&step.action)?
                }
            };
            results.push(result.clone());
            self.planner.history.record(step.action.clone(), result, true);
        }
        Ok(results.join("\n"))
    }

    pub fn tools(&self) -> Vec<&Tool> {
        self.registry.list()
    }

    pub fn status(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "tools": self.registry.list().len(),
            "success_rate": self.planner.history.success_rate(),
            "observations": self.planner.memory.observations.len(),
        })
    }
}
