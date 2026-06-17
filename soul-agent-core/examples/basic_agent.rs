//! # Basic Agent Example
//!
//! Demonstrates how to create a simple agent using the SoulSystem framework.
//!
//! ```bash
//! cargo run --example basic_agent -p soul_agent_core
//! ```

use soul_agent_core::builder::{AgentBuilder, AgentConfig, ComposedAgent};
use soul_agent_core::traits::*;
use std::sync::atomic::{AtomicU64, Ordering};

// ── Custom LLM Implementation ──────────────────────────────────────

struct MockLLM {
    model_name: String,
    call_count: AtomicU64,
}

impl MockLLM {
    fn new(model_name: &str) -> Self {
        Self {
            model_name: model_name.to_string(),
            call_count: AtomicU64::new(0),
        }
    }
}

#[async_trait::async_trait]
impl LLMClient for MockLLM {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        _tools: Option<&[ToolSchema]>,
    ) -> Result<String, LLMError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);

        // Get the last user message
        let last_user_msg = messages
            .iter()
            .rev()
            .find(|m| m.role == ChatRole::User)
            .map(|m| m.content.as_str())
            .unwrap_or("no message");

        // Simple echo response based on user input
        if last_user_msg.contains("Task completed") || last_user_msg.contains("done") {
            Ok("Task completed successfully.".into())
        } else if last_user_msg.contains("error") {
            Ok("Task completed with errors.".into())
        } else {
            Ok(format!("I will process: {}", last_user_msg))
        }
    }

    async fn generate(&self, prompt: &str) -> Result<String, LLMError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(format!("Generated response for: {}", prompt))
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }
}

// ── Mock Memory ─────────────────────────────────────────────────────

struct MockMemory;

#[async_trait::async_trait]
impl Memory for MockMemory {
    async fn store(&self, _record: MemoryRecord) -> Result<(), MemoryError> {
        Ok(())
    }

    async fn search(&self, _query: &str, _k: usize) -> Result<Vec<MemorySearchResult>, MemoryError> {
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

// ── Custom Tool Implementation ──────────────────────────────────────

struct EchoTool;

#[async_trait::async_trait]
impl Tool for EchoTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "echo".to_string(),
            description: "Echoes the input back".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "Message to echo"
                    }
                },
                "required": ["message"]
            }),
        }
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError> {
        let message = args
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("no message");
        Ok(ToolResult {
            output: format!("Echo: {}", message),
            success: true,
            duration_ms: 0,
        })
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Read
    }
}

// ── Main ────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    println!("=== SoulSystem Framework - Basic Agent Example ===\n");

    // Create components
    let llm = MockLLM::new("mock-model-v1");
    let tool = EchoTool;
    let memory = MockMemory;
    let planner = MockPlanner;

    // Build agent with builder pattern
    let mut agent: ComposedAgent<MockLLM, MockMemory, EchoTool, MockPlanner> = AgentBuilder::new()
        .llm(llm)
        .memory(memory)
        .tool(tool)
        .planner(planner)
        .name("ExampleBot")
        .max_turns(5)
        .config(AgentConfig {
            safety_warning_turns: vec![3],
            ..Default::default()
        })
        .build()
        .expect("Failed to build agent");

    // Run a task
    println!("Running task: 'List files in /tmp'");
    match agent.run_task("List files in /tmp").await {
        Ok(result) => println!("Result: {}", result),
        Err(e) => println!("Error: {}", e),
    }

    // Ask a question
    println!("\nAsking: 'What can you do?'");
    match agent.ask("What can you do?").await {
        Ok(response) => println!("Response: {}", response),
        Err(e) => println!("Error: {}", e),
    }

    // Check status
    println!("\nAgent Status:");
    println!("{}", serde_json::to_string_pretty(&agent.status()).unwrap());
}
