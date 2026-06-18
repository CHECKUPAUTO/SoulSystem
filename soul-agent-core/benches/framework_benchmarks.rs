//! # SoulSystem Framework Benchmarks
//!
//! Compares SoulSystem performance with typical Python framework overhead.
//!
//! Run benchmarks:
//! ```bash
//! cargo bench -p soul_agent_core
//! ```

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use soul_agent_core::builder::{AgentBuilder, ComposedAgent};
use soul_agent_core::traits::*;
use std::sync::atomic::{AtomicU64, Ordering};

// ── Mock Implementations ──────────────────────────────────────────────

struct MockLLM {
    call_count: AtomicU64,
}

impl MockLLM {
    fn new() -> Self {
        Self {
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
        let last_user_msg = messages
            .iter()
            .rev()
            .find(|m| m.role == ChatRole::User)
            .map(|m| m.content.as_str())
            .unwrap_or("no message");
        Ok(format!("Response to: {}", last_user_msg))
    }

    async fn generate(&self, prompt: &str) -> Result<String, LLMError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(format!("Generated: {}", prompt))
    }

    fn model_name(&self) -> &str {
        "mock-llm"
    }
}

struct MockMemory {
    store_count: AtomicU64,
    search_count: AtomicU64,
}

impl MockMemory {
    fn new() -> Self {
        Self {
            store_count: AtomicU64::new(0),
            search_count: AtomicU64::new(0),
        }
    }
}

#[async_trait::async_trait]
impl Memory for MockMemory {
    async fn store(&self, _record: MemoryRecord) -> Result<(), MemoryError> {
        self.store_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn search(
        &self,
        _query: &str,
        _k: usize,
    ) -> Result<Vec<MemorySearchResult>, MemoryError> {
        self.search_count.fetch_add(1, Ordering::SeqCst);
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

struct MockTool {
    execute_count: AtomicU64,
}

impl MockTool {
    fn new() -> Self {
        Self {
            execute_count: AtomicU64::new(0),
        }
    }
}

#[async_trait::async_trait]
impl Tool for MockTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "mock_tool".into(),
            description: "Mock tool for benchmarking".into(),
            parameters: serde_json::json!({}),
        }
    }

    async fn execute(&self, _args: serde_json::Value) -> Result<ToolResult, ToolError> {
        self.execute_count.fetch_add(1, Ordering::SeqCst);
        Ok(ToolResult {
            output: "executed".into(),
            success: true,
            duration_ms: 0,
        })
    }
}

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

// ── Benchmarks ──────────────────────────────────────────────────────

/// Benchmark: Agent creation overhead
fn bench_agent_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("agent_creation");

    group.bench_function("soul_system_builder", |b| {
        b.iter(|| {
            let _: ComposedAgent<MockLLM, MockMemory, MockTool, MockPlanner> = AgentBuilder::new()
                .llm(MockLLM::new())
                .memory(MockMemory::new())
                .tool(MockTool::new())
                .planner(MockPlanner)
                .name("BenchAgent")
                .build()
                .unwrap();
        });
    });

    group.finish();
}

/// Benchmark: LLM chat call overhead
fn bench_llm_chat(c: &mut Criterion) {
    let mut group = c.benchmark_group("llm_chat");

    let rt = tokio::runtime::Runtime::new().unwrap();

    group.bench_function("soul_system_chat", |b| {
        let mut agent: ComposedAgent<MockLLM, MockMemory, MockTool, MockPlanner> =
            AgentBuilder::new()
                .llm(MockLLM::new())
                .memory(MockMemory::new())
                .tool(MockTool::new())
                .planner(MockPlanner)
                .build()
                .unwrap();

        b.iter(|| {
            rt.block_on(async {
                agent.ask("benchmark question").await.unwrap();
            });
        });
    });

    group.finish();
}

/// Benchmark: Tool execution overhead
fn bench_tool_execution(c: &mut Criterion) {
    let mut group = c.benchmark_group("tool_execution");

    let rt = tokio::runtime::Runtime::new().unwrap();

    group.bench_function("soul_system_tool", |b| {
        let tool = MockTool::new();

        b.iter(|| {
            rt.block_on(async {
                tool.execute(serde_json::json!({})).await.unwrap();
            });
        });
    });

    group.finish();
}

/// Benchmark: Memory store overhead
fn bench_memory_store(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_store");

    let rt = tokio::runtime::Runtime::new().unwrap();

    group.bench_function("soul_system_store", |b| {
        let memory = MockMemory::new();

        b.iter(|| {
            rt.block_on(async {
                memory
                    .store(MemoryRecord {
                        id: String::new(),
                        text: "benchmark memory entry".into(),
                        importance: 0.5,
                        tags: vec!["benchmark".into()],
                        metadata: std::collections::HashMap::new(),
                        created_at: "2024-01-15".into(),
                    })
                    .await
                    .unwrap();
            });
        });
    });

    group.finish();
}

/// Benchmark: Memory search overhead
fn bench_memory_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_search");

    let rt = tokio::runtime::Runtime::new().unwrap();

    group.bench_function("soul_system_search", |b| {
        let memory = MockMemory::new();

        b.iter(|| {
            rt.block_on(async {
                memory.search("benchmark query", 10).await.unwrap();
            });
        });
    });

    group.finish();
}

/// Benchmark: Full ReAct loop (multiple turns)
fn bench_react_loop(c: &mut Criterion) {
    let mut group = c.benchmark_group("react_loop");

    let rt = tokio::runtime::Runtime::new().unwrap();

    group.bench_function("soul_system_react_5_turns", |b| {
        b.iter(|| {
            let mut agent: ComposedAgent<MockLLM, MockMemory, MockTool, MockPlanner> =
                AgentBuilder::new()
                    .llm(MockLLM::new())
                    .memory(MockMemory::new())
                    .tool(MockTool::new())
                    .planner(MockPlanner)
                    .max_turns(5)
                    .build()
                    .unwrap();

            rt.block_on(async {
                agent.run_task("benchmark task").await.unwrap();
            });
        });
    });

    group.finish();
}

/// Benchmark: Concurrent agent operations
fn bench_concurrent_agents(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_agents");

    let rt = tokio::runtime::Runtime::new().unwrap();

    for num_agents in [1, 5, 10, 20] {
        group.bench_with_input(
            BenchmarkId::new("soul_system_agents", num_agents),
            &num_agents,
            |b, &num_agents| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut handles = Vec::new();

                        for i in 0..num_agents {
                            let mut agent: ComposedAgent<
                                MockLLM,
                                MockMemory,
                                MockTool,
                                MockPlanner,
                            > = AgentBuilder::new()
                                .llm(MockLLM::new())
                                .memory(MockMemory::new())
                                .tool(MockTool::new())
                                .planner(MockPlanner)
                                .name(&format!("Agent_{}", i))
                                .build()
                                .unwrap();

                            handles.push(tokio::spawn(async move {
                                agent.run_task("concurrent task").await.unwrap();
                            }));
                        }

                        for handle in handles {
                            handle.await.unwrap();
                        }
                    });
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Context compaction overhead
fn bench_context_compaction(c: &mut Criterion) {
    let mut group = c.benchmark_group("context_compaction");

    let rt = tokio::runtime::Runtime::new().unwrap();

    group.bench_function("soul_system_compact", |b| {
        b.iter(|| {
            let mut agent: ComposedAgent<MockLLM, MockMemory, MockTool, MockPlanner> =
                AgentBuilder::new()
                    .llm(MockLLM::new())
                    .memory(MockMemory::new())
                    .tool(MockTool::new())
                    .planner(MockPlanner)
                    .build()
                    .unwrap();

            // Fill context with messages
            for i in 0..100 {
                agent.push_user_message(&format!("Message {}", i));
                agent.push_assistant_message(&format!("Response {}", i));
            }

            rt.block_on(async {
                // This will trigger compaction if context is large
                agent.run_task("trigger compaction").await.unwrap();
            });
        });
    });

    group.finish();
}

// ── Criterion Group ─────────────────────────────────────────────────

criterion_group!(
    benches,
    bench_agent_creation,
    bench_llm_chat,
    bench_tool_execution,
    bench_memory_store,
    bench_memory_search,
    bench_react_loop,
    bench_concurrent_agents,
    bench_context_compaction,
);

criterion_main!(benches);
