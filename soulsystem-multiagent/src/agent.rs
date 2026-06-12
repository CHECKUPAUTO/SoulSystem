use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a specialized agent with capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecializedAgent {
    pub id: String,
    pub name: String,
    pub capabilities: Vec<AgentCapability>,
    pub endpoint: String,
    pub token: String,
}

/// Capability descriptor for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapability {
    pub name: String,
    pub description: String,
    pub priority: u32,
}

/// A task submitted for processing by an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    pub id: String,
    pub name: String,
    pub task_type: String,
    pub payload: HashMap<String, serde_json::Value>,
    pub priority: u32,
}

impl AgentTask {
    pub fn new(name: &str, task_type: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            task_type: task_type.to_string(),
            payload: HashMap::new(),
            priority: 0,
        }
    }
}

/// Result of executing a task against an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub success: bool,
    pub output: String,
    pub confidence: f64,
}
