use soul_agent_core::{AgentConfig, AutonomousAgent};
use soul_llm::{LlmConfig, OllamaClient};

pub struct AutonomousEntity {
    pub agent: AutonomousAgent,
    pub name: String,
}

impl AutonomousEntity {
    pub fn new(config: LlmConfig, name: &str) -> Self {
        let agent_config = AgentConfig {
            name: name.to_string(),
            ..Default::default()
        };
        let agent = AutonomousAgent::new(OllamaClient::new(config), agent_config);
        Self {
            agent,
            name: name.to_string(),
        }
    }

    pub async fn is_alive(&self) -> bool {
        // Legacy shim does not expose a health check; assume alive.
        true
    }

    pub async fn ask(&mut self, prompt: &str) -> Result<String, String> {
        self.agent.ask(prompt).await
    }

    pub async fn run_task(&mut self, task: &str) -> Result<String, String> {
        self.agent.run_task(task).await
    }

    pub fn status(&self) -> serde_json::Value {
        self.agent.status()
    }

    pub fn tools(&self) -> Vec<&soul_tools::Tool> {
        self.agent.registry.list().to_vec()
    }
}
