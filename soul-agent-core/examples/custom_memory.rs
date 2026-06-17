//! # Custom Memory Backend Example
//!
//! Demonstrates how to implement a custom memory backend for the framework.
//!
//! ```bash
//! cargo run --example custom_memory -p soul_agent_core
//! ```

use soul_agent_core::builder::{AgentBuilder, ComposedAgent};
use soul_agent_core::traits::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

// ── In-Memory Memory Backend ────────────────────────────────────────

struct InMemoryBackend {
    records: RwLock<Vec<MemoryRecord>>,
    next_id: AtomicU64,
}

impl InMemoryBackend {
    fn new() -> Self {
        Self {
            records: RwLock::new(Vec::new()),
            next_id: AtomicU64::new(1),
        }
    }
}

#[async_trait::async_trait]
impl Memory for InMemoryBackend {
    async fn store(&self, mut record: MemoryRecord) -> Result<(), MemoryError> {
        record.id = self.next_id.fetch_add(1, Ordering::SeqCst).to_string();
        self.records
            .write()
            .map_err(|e| MemoryError::StorageError(e.to_string()))?
            .push(record);
        Ok(())
    }

    async fn search(
        &self,
        query: &str,
        k: usize,
    ) -> Result<Vec<MemorySearchResult>, MemoryError> {
        let records = self
            .records
            .read()
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        // Simple search: return records whose text contains the query
        let query_lower = query.to_lowercase();
        let mut results: Vec<MemorySearchResult> = records
            .iter()
            .filter(|r| r.text.to_lowercase().contains(&query_lower))
            .map(|r| MemorySearchResult {
                record: r.clone(),
                score: 0.8, // Mock relevance score
            })
            .collect();

        results.truncate(k);
        Ok(results)
    }

    async fn get_context(&self, query: &str) -> Result<String, MemoryError> {
        let results = self.search(query, 5).await?;
        if results.is_empty() {
            return Ok(String::new());
        }

        let ctx: Vec<String> = results
            .into_iter()
            .enumerate()
            .map(|(i, r)| {
                format!(
                    "[{}] (score: {:.2}): {}",
                    i + 1,
                    r.score,
                    r.record.text
                )
            })
            .collect();

        Ok(ctx.join("\n"))
    }

    fn decay_and_prune(
        &self,
        decay_factor: f32,
        threshold: f32,
        max_entries: usize,
    ) -> Result<(usize, usize), MemoryError> {
        let mut records = self
            .records
            .write()
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        // Apply decay
        for record in records.iter_mut() {
            record.importance *= decay_factor;
        }

        // Remove below threshold
        let initial_len = records.len();
        records.retain(|r| r.importance >= threshold);
        let removed = initial_len - records.len();

        // Trim to max entries
        if records.len() > max_entries {
            records.sort_by(|a, b| {
                a.importance
                    .partial_cmp(&b.importance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let to_remove = records.len() - max_entries;
            records.drain(0..to_remove);
        }

        let kept = records.len();
        Ok((kept, removed))
    }

    fn count(&self) -> usize {
        self.records.read().map(|r| r.len()).unwrap_or(0)
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

        Ok(format!("I remember: {}", last_user_msg))
    }

    async fn generate(&self, prompt: &str) -> Result<String, LLMError> {
        Ok(format!("Generated: {}", prompt))
    }

    fn model_name(&self) -> &str {
        "mock-model"
    }
}

// ── Mock Tool ───────────────────────────────────────────────────────

struct MockTool;

#[async_trait::async_trait]
impl Tool for MockTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "mock".to_string(),
            description: "Mock tool".to_string(),
            parameters: serde_json::json!({}),
        }
    }

    async fn execute(&self, _args: serde_json::Value) -> Result<ToolResult, ToolError> {
        Ok(ToolResult {
            output: "mock executed".into(),
            success: true,
            duration_ms: 0,
        })
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
    println!("=== SoulSystem Framework - Custom Memory Backend Example ===\n");

    // Create memory backend
    let memory = InMemoryBackend::new();
    println!("Created in-memory backend with {} entries", memory.count());

    // Store some memories
    println!("\nStoring memories...");
    memory
        .store(MemoryRecord {
            id: String::new(),
            text: "SoulSystem is a Rust monorepo with 198 crates".into(),
            importance: 0.9,
            tags: vec!["project".into(), "architecture".into()],
            metadata: HashMap::new(),
            created_at: "2024-01-15".into(),
        })
        .await
        .unwrap();

    memory
        .store(MemoryRecord {
            id: String::new(),
            text: "The autonomous agent uses ReAct loop pattern".into(),
            importance: 0.85,
            tags: vec!["agent".into(), "pattern".into()],
            metadata: HashMap::new(),
            created_at: "2024-01-15".into(),
        })
        .await
        .unwrap();

    memory
        .store(MemoryRecord {
            id: String::new(),
            text: "Memory has hierarchical layers: working, episodic, semantic".into(),
            importance: 0.8,
            tags: vec!["memory".into(), "architecture".into()],
            metadata: HashMap::new(),
            created_at: "2024-01-15".into(),
        })
        .await
        .unwrap();

    println!("Stored {} memories", memory.count());

    // Search memories
    println!("\nSearching for 'agent':");
    match memory.get_context("agent").await {
        Ok(ctx) => println!("Context:\n{}", ctx),
        Err(e) => println!("Error: {:?}", e),
    }

    // Build agent with memory
    let mut agent: ComposedAgent<MockLLM, InMemoryBackend, MockTool, MockPlanner> =
        AgentBuilder::new()
            .llm(MockLLM)
            .memory(memory)
            .tool(MockTool)
            .planner(MockPlanner)
            .name("MemoryBot")
            .max_turns(2)
            .build()
            .expect("Failed to build agent");

    // Run a task that uses memory
    println!("\nRunning task: 'Tell me about the project'");
    match agent.run_task("Tell me about the project").await {
        Ok(result) => println!("Result: {}", result),
        Err(e) => println!("Error: {}", e),
    }

    // Check status
    println!("\nAgent Status:");
    println!("{}", serde_json::to_string_pretty(&agent.status()).unwrap());
}
