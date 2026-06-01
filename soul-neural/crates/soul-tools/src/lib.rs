use soul_core::{Tool, Goal};
use soul_embed::Embedder;
use std::collections::HashMap;
use anyhow::Result;

pub struct ToolOrchestrator {
    tools: HashMap<String, Box<dyn Tool>>,
    #[allow(dead_code)]
    embedder: Embedder,
    recipes: HashMap<String, Vec<String>>,
}

impl ToolOrchestrator {
    pub fn new(embedder: Embedder) -> Self {
        Self { tools: HashMap::new(), embedder, recipes: HashMap::new() }
    }

    pub fn register_tool(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub async fn plan_for_goal(&mut self, goal: &Goal) -> Result<Vec<String>> {
        if let Some(recipe) = self.recipes.get(&goal.description) {
            return Ok(recipe.clone());
        }
        Ok(vec![])
    }

    pub async fn execute_sequence(&self, sequence: &[String], params: &serde_json::Value) -> Result<Vec<serde_json::Value>> {
        let mut results = Vec::new();
        for tool_name in sequence {
            if let Some(tool) = self.tools.get(tool_name) {
                let res = tool.execute(params).map_err(|e| anyhow::anyhow!(e))?;
                results.push(res);
            } else {
                return Err(anyhow::anyhow!("Tool not found: {}", tool_name));
            }
        }
        Ok(results)
    }

    pub fn learn_from_success(&mut self, sequence: &[String], goal_desc: &str) {
        self.recipes.insert(goal_desc.to_string(), sequence.to_vec());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soul_embed::Embedder;

    #[test]
    fn test_orchestrator_creation() {
        let embedder = Embedder::new(&["test"]);
        let mut o = ToolOrchestrator::new(embedder);
        // plan_for_goal on a fresh orchestrator returns empty
        let goal = Goal {
            id: "test".into(),
            description: "unknown".into(),
            embedding: vec![],
            priority: 0.5,
            created_at: 0,
            deadline: None,
            status: soul_core::GoalStatus::Active,
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let seq = rt.block_on(o.plan_for_goal(&goal)).unwrap();
        assert!(seq.is_empty(), "Fresh orchestrator should return empty plan");
    }
}
