# SoulSystem Framework Guide

## Overview

SoulSystem is a Rust framework for building autonomous AI agents. It provides a set of traits and components that can be composed together to create custom agents with different capabilities.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                    Agent                            │
│  (run_task, ask, abort, status)                    │
├─────────────────────────────────────────────────────┤
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐ │
│  │   LLM   │  │ Memory  │  │  Tools  │  │ Planner │ │
│  │ Client  │  │Backend  │  │         │  │         │ │
│  └─────────┘  └─────────┘  └─────────┘  └─────────┘ │
└─────────────────────────────────────────────────────┘
```

## Core Traits

### LLMClient

The `LLMClient` trait defines the interface for LLM providers.

```rust
use soul_agent_core::traits::{LLMClient, ChatMessage, ToolSchema};

#[async_trait]
trait LLMClient: Send + Sync {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolSchema]>,
    ) -> Result<String, LLMError>;

    async fn generate(&self, prompt: &str) -> Result<String, LLMError>;

    fn model_name(&self) -> &str;
}
```

### Memory

The `Memory` trait defines the interface for memory backends.

```rust
use soul_agent_core::traits::{Memory, MemoryRecord, MemorySearchResult};

#[async_trait]
trait Memory: Send + Sync {
    async fn store(&self, record: MemoryRecord) -> Result<(), MemoryError>;

    async fn search(&self, query: &str, k: usize) -> Result<Vec<MemorySearchResult>, MemoryError>;

    async fn get_context(&self, query: &str) -> Result<String, MemoryError>;

    fn decay_and_prune(
        &self,
        decay_factor: f32,
        threshold: f32,
        max_entries: usize,
    ) -> Result<(usize, usize), MemoryError>;

    fn count(&self) -> usize;
}
```

### Tool

The `Tool` trait defines the interface for custom tools.

```rust
use soul_agent_core::traits::{Tool, ToolSchema, ToolResult, PermissionLevel};

#[async_trait]
trait Tool: Send + Sync {
    fn schema(&self) -> ToolSchema;

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError>;

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Read
    }
}
```

### Planner

The `Planner` trait defines the interface for goal decomposition.

```rust
use soul_agent_core::traits::{Planner, Goal, Plan};

#[async_trait]
trait Planner: Send + Sync {
    async fn create_plan(&self, goal: &Goal) -> Result<Plan, PlannerError>;

    async fn decide(&self, context: &str) -> Result<String, PlannerError>;

    fn evaluate(&self, step: &PlanStep, outcome: &str) -> f32;
}
```

### Agent

The `Agent` trait defines the high-level agent interface.

```rust
use soul_agent_core::traits::Agent;

#[async_trait]
trait Agent: Send + Sync {
    async fn run_task(&mut self, task: &str) -> Result<String, AgentError>;

    async fn ask(&mut self, question: &str) -> Result<String, AgentError>;

    async fn abort(&self);

    fn status(&self) -> serde_json::Value;
}
```

## Builder Pattern

The `AgentBuilder` provides a fluent API for composing agents.

```rust
use soul_agent_core::builder::{AgentBuilder, AgentConfig};

let agent = AgentBuilder::new()
    .llm(my_llm_client)
    .memory(my_memory)
    .tool(my_tool)
    .planner(my_planner)
    .name("MyAgent")
    .max_turns(10)
    .config(AgentConfig {
        safety_warning_turns: vec![5, 10],
        ..Default::default()
    })
    .build()?;
```

## Examples

### Basic Agent

A minimal agent with a mock LLM and custom tool.

```bash
cargo run --example basic_agent -p soul_agent_core
```

### Custom Tool

Demonstrates implementing custom tools with different permission levels.

```bash
cargo run --example custom_tool -p soul_agent_core
```

### Custom Memory Backend

Shows how to implement a custom memory backend.

```bash
cargo run --example custom_memory -p soul_agent_core
```

## Permission Levels

Tools can have different permission levels:

- `Read` - Read-only operations (safe)
- `Write` - Write operations (may modify state)
- `Destructive` - Destructive operations (requires explicit confirmation)

## Error Handling

All traits use custom error types that implement `std::error::Error`:

- `LLMError` - Network, timeout, rate limiting errors
- `MemoryError` - Storage, serialization errors
- `ToolError` - Execution, permission, argument errors
- `PlannerError` - Planning, decision errors
- `AgentError` - Task, LLM, tool, abort errors

## Integration with SoulSystem

The framework integrates with the broader SoulSystem ecosystem:

- `soul_llm` - Built-in LLM client implementations
- `soul_memory` - Vector storage with sled backend
- `soul_tools` - System tools and sandbox
- `soul_planner` - Cognitive loop and planning
- `soul_sandbox` - Secure execution environment

## Next Steps

1. Read the `examples/` directory for working code
2. Check `docs/` for additional documentation
3. Review `src/traits.rs` for the complete trait definitions
