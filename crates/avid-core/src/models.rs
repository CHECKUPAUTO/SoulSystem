use garde::Validate;
use serde::{Deserialize, Serialize};

/// Task types for specialized agent routing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskType {
    General,
    Coding,
    Infrastructure,
    SQL,
    Security,
    Debugging,
    Testing,
    Documentation,
    Research,
}

/// A task request submitted via the API.
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct TaskRequest {
    #[garde(length(min = 1, max = 4096))]
    pub task: String,
    #[garde(skip)]
    pub task_type: Option<TaskType>,
    #[garde(skip)]
    pub url: Option<String>,
    #[garde(skip)]
    pub metadata: Option<serde_json::Value>,
}

/// A task result returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub status: String,
    pub plan: Option<Plan>,
    pub submission: Option<avid_anticlone::Submission>,
    pub execution: Option<ExecutionSummary>,
    pub originality: Option<f32>,
    pub created_at: i64,
}

/// A plan produced by the Planner agent.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Plan {
    #[garde(length(min = 1, max = 2000))]
    pub goal: String,
    #[garde(length(min = 1, max = 10), dive)]
    #[serde(alias = "steps")]
    pub decomposition: Vec<Step>,
    #[garde(length(max = 20))]
    pub constraints: Vec<String>,
    #[garde(length(min = 1, max = 5))]
    pub success_criteria: Vec<String>,
}

/// A single step in the plan decomposition.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Step {
    #[garde(length(min = 1, max = 500))]
    pub description: String,
    #[garde(length(min = 1, max = 200))]
    pub output: String,
}

/// A module within a code submission.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Module {
    #[garde(length(min = 1, max = 500))]
    pub path: String,
    #[garde(length(min = 1, max = 500))]
    pub purpose: String,
}

/// Summary of sandbox execution for the API response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSummary {
    pub status: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

/// A directed acyclic graph representing business logic flows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogicGraph {
    pub nodes: Vec<LogicNode>,
    pub edges: Vec<LogicEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogicNode {
    pub id: String,
    pub action: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogicEdge {
    pub source: String,
    pub target: String,
    pub condition: Option<String>,
}

/// Inferred data structures from Vision or Cortex analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaModel {
    pub types: std::collections::HashMap<String, String>,
    pub constraints: Vec<String>,
}
