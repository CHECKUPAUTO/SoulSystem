//! # Custom Tool Example
//!
//! Demonstrates how to implement and register a custom tool with the framework.
//!
//! ```bash
//! cargo run --example custom_tool -p soul_agent_core
//! ```

use soul_agent_core::builder::{AgentBuilder, ComposedAgent};
use soul_agent_core::traits::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

// ── Custom Tool: File Counter ───────────────────────────────────────

struct FileCounterTool {
    count: AtomicU64,
}

impl FileCounterTool {
    fn new() -> Self {
        Self {
            count: AtomicU64::new(0),
        }
    }
}

#[async_trait::async_trait]
impl Tool for FileCounterTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "count_files".to_string(),
            description: "Counts files in a directory".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path to count files in"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments("missing 'path' argument".into()))?;

        let start = Instant::now();

        // Simple file counting (not recursive)
        let count = std::fs::read_dir(path)
            .map_err(|e| ToolError::ExecutionError(e.to_string()))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().map(|ft| ft.is_file()).unwrap_or(false))
            .count();

        self.count.fetch_add(1, Ordering::SeqCst);
        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(ToolResult {
            output: format!("Found {} files in {}", count, path),
            success: true,
            duration_ms,
        })
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Read
    }
}

// ── Mock LLM ────────────────────────────────────────────────────────

struct MockLLM;

#[async_trait::async_trait]
impl LLMClient for MockLLM {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        _tools: Option<&[ToolSchema]>,
    ) -> Result<String, LLMError> {
        let last_user_msg = messages
            .iter()
            .rev()
            .find(|m| m.role == ChatRole::User)
            .map(|m| m.content.as_str())
            .unwrap_or("");

        Ok(format!("Processing your request: {}", last_user_msg))
    }

    async fn generate(&self, prompt: &str) -> Result<String, LLMError> {
        Ok(format!("Generated: {}", prompt))
    }

    fn model_name(&self) -> &str {
        "mock-model"
    }
}

// ── Mock Memory ─────────────────────────────────────────────────────

struct MockMemory;

#[async_trait::async_trait]
impl Memory for MockMemory {
    async fn store(&self, _record: MemoryRecord) -> Result<(), MemoryError> {
        Ok(())
    }

    async fn search(
        &self,
        _query: &str,
        _k: usize,
    ) -> Result<Vec<MemorySearchResult>, MemoryError> {
        Ok(vec![])
    }

    async fn get_context(&self, _query: &str) -> Result<String, MemoryError> {
        Ok(String::new())
    }

    fn decay_and_prune(
        &self,
        _decay_factor: f32,
        _threshold: f32,
        _max_entries: usize,
    ) -> Result<(usize, usize), MemoryError> {
        Ok((0, 0))
    }

    fn count(&self) -> usize {
        0
    }
}

// ── Mock Planner ────────────────────────────────────────────────────

struct MockPlanner;

#[async_trait::async_trait]
impl Planner for MockPlanner {
    async fn create_plan(&self, _goal: &Goal) -> Result<Plan, PlannerError> {
        Ok(Plan {
            id: "mock-plan".into(),
            goal_id: "mock-goal".into(),
            steps: vec![],
        })
    }

    async fn decide(&self, _context: &str) -> Result<String, PlannerError> {
        Ok("continue".into())
    }

    fn evaluate(&self, _step: &PlanStep, _outcome: &str) -> f32 {
        0.5
    }
}

// ── Main ────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    println!("=== SoulSystem Framework - Custom Tool Example ===\n");

    // Create tool
    let file_counter = FileCounterTool::new();

    println!("Registered tool:");
    println!(
        "  {} - {}",
        file_counter.schema().name,
        file_counter.schema().description
    );

    // Build agent with tool
    let mut agent: ComposedAgent<MockLLM, MockMemory, FileCounterTool, MockPlanner> =
        AgentBuilder::new()
            .llm(MockLLM)
            .memory(MockMemory)
            .tool(file_counter)
            .planner(MockPlanner)
            .name("ToolDemoBot")
            .max_turns(3)
            .build()
            .expect("Failed to build agent");

    // Run a task
    println!("\nRunning task: 'Count files in current directory'");
    match agent.run_task("Count files in current directory").await {
        Ok(result) => println!("Result: {}", result),
        Err(e) => println!("Error: {}", e),
    }

    // Check status
    println!("\nAgent Status:");
    println!("{}", serde_json::to_string_pretty(&agent.status()).unwrap());
}
