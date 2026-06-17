# SoulSystem Framework API Reference

## Overview

This document provides a comprehensive API reference for the SoulSystem framework traits and components.

## Module: `soul_agent_core::traits`

### `LLMClient` Trait

Multi-provider LLM client interface.

```rust
#[async_trait]
pub trait LLMClient: Send + Sync {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolSchema]>,
    ) -> Result<String, LLMError>;

    async fn generate(&self, prompt: &str) -> Result<String, LLMError>;

    fn model_name(&self) -> &str;
}
```

**Methods:**

- `chat(messages, tools)` - Send messages to the LLM and get a response
- `generate(prompt)` - Generate text from a prompt (no tool calling)
- `model_name()` - Get the model name/identifier

### `Memory` Trait

Hierarchical memory interface.

```rust
#[async_trait]
pub trait Memory: Send + Sync {
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

**Methods:**

- `store(record)` - Store a memory record
- `search(query, k)` - Search for relevant memories
- `get_context(query)` - Get formatted context for prompt injection
- `decay_and_prune(factor, threshold, max)` - Decay old memories and prune
- `count()` - Get total memory count

### `Tool` Trait

Tool interface with schema and execution.

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn schema(&self) -> ToolSchema;

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError>;

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Read
    }
}
```

**Methods:**

- `schema()` - Get tool schema (name, description, parameters)
- `execute(args)` - Execute the tool with given arguments
- `permission_level()` - Get required permission level (default: Read)

### `Planner` Trait

Goal decomposition and decision making.

```rust
#[async_trait]
pub trait Planner: Send + Sync {
    async fn create_plan(&self, goal: &Goal) -> Result<Plan, PlannerError>;

    async fn decide(&self, context: &str) -> Result<String, PlannerError>;

    fn evaluate(&self, step: &PlanStep, outcome: &str) -> f32;
}
```

**Methods:**

- `create_plan(goal)` - Create a plan from a goal
- `decide(context)` - Decide next action based on context
- `evaluate(step, outcome)` - Evaluate a plan step outcome

### `Agent` Trait

High-level agent interface.

```rust
#[async_trait]
pub trait Agent: Send + Sync {
    async fn run_task(&mut self, task: &str) -> Result<String, AgentError>;

    async fn ask(&mut self, question: &str) -> Result<String, AgentError>;

    async fn abort(&self);

    fn status(&self) -> serde_json::Value;
}
```

**Methods:**

- `run_task(task)` - Execute a task using ReAct loop
- `ask(question)` - Ask a question (conversation mode)
- `abort()` - Abort the current running task
- `status()` - Get agent status as JSON

## Module: `soul_agent_core::builder`

### `AgentBuilder` Struct

Builder pattern for composing agents.

```rust
pub struct AgentBuilder<L: LLMClient, M: Memory, T: Tool, P: Planner> {
    // private fields
}
```

**Methods:**

- `new()` - Create a new builder
- `llm(client)` - Set the LLM client
- `memory(mem)` - Set the memory backend
- `tool(t)` - Add a tool
- `planner(p)` - Set the planner
- `name(name)` - Set agent name
- `max_turns(n)` - Set max turns for ReAct loop
- `max_context_chars(n)` - Set max context characters
- `config(cfg)` - Set agent configuration
- `build()` - Build the agent (requires all components)

### `ComposedAgent` Struct

Agent composed from trait objects.

```rust
pub struct ComposedAgent<L: LLMClient, M: Memory, T: Tool, P: Planner> {
    // private fields
}
```

Implements `Agent` trait.

### `AgentConfig` Struct

Agent configuration.

```rust
pub struct AgentConfig {
    pub name: String,
    pub max_turns: usize,
    pub max_tool_retries: usize,
    pub shell_timeout_secs: u64,
    pub safety_warning_turns: Vec<usize>,
    pub auto_distill: bool,
    pub working_memory_capacity: usize,
    pub max_context_chars: usize,
}
```

### `PluginRegistry` Struct

Dynamic plugin loading for tools and memory backends.

```rust
pub struct PluginRegistry {
    // private fields
}
```

**Methods:**

- `new()` - Create a new registry
- `register_tool(name, tool)` - Register a tool
- `get_tool(name)` - Get a tool by name
- `list_tools()` - List all registered tool names
- `register_memory(name, memory)` - Register a memory backend
- `get_memory(name)` - Get a memory backend by name
- `list_memory_backends()` - List all registered memory backends

## Module: `soul_agent_core`

### `AutonomousAgent` Struct

Built-in agent implementation with full ReAct loop.

```rust
pub struct AutonomousAgent {
    pub config: AgentConfig,
    pub llm: OllamaClient,
    pub chat_session: ChatSession,
    pub planner: CognitiveLoop,
    pub registry: ToolRegistry,
    pub executor: AsyncShellExecutor,
    pub tool_schemas: Vec<ToolSchema>,
    pub history: Vec<String>,
    pub turn: usize,
    pub consecutive_failures: usize,
    pub repair_count: usize,
    pub running: Arc<RwLock<bool>>,
    pub memory: Arc<HierarchicalMemory>,
    pub metacognition: Arc<MetaCognition>,
    pub reasoning: ThoughtTree,
    pub trajectory_recorder: Option<TrajectoryRecorder>,
    pub knowledge_graph: KnowledgeGraph,
    pub skill_loader: Option<Arc<RwLock<SkillLoader>>>,
    event_tx: Option<mpsc::UnboundedSender<AgentEvent>>,
}
```

**Methods:**

- `new(llm, config)` - Create a new agent
- `with_memory(llm, config, memory)` - Create with shared memory
- `set_event_sender(tx)` - Set event streaming sender
- `set_skill_loader(loader)` - Set skill loader
- `run_task(task)` - Execute a task
- `ask(question)` - Ask a question
- `abort()` - Abort current task
- `auto_repair()` - Trigger auto-repair
- `repair_count()` - Get repair count
- `status()` - Get agent status

## Error Types

### `LLMError`

```rust
pub enum LLMError {
    Network(String),
    Timeout(String),
    RateLimited(String),
    InvalidRequest(String),
    Unknown(String),
}
```

### `MemoryError`

```rust
pub enum MemoryError {
    StorageError(String),
    SerializationError(String),
    NotFound(String),
}
```

### `ToolError`

```rust
pub enum ToolError {
    ExecutionError(String),
    InvalidArguments(String),
    PermissionDenied(String),
}
```

### `PlannerError`

```rust
pub enum PlannerError {
    PlanningFailed(String),
    DecisionFailed(String),
    LLMError(String),
}
```

### `AgentError`

```rust
pub enum AgentError {
    TaskFailed(String),
    LLMError(String),
    ToolError(String),
    Aborted,
}
```

## Data Types

### `ChatMessage`

```rust
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    pub tool_calls: Option<Vec<ToolCallInfo>>,
    pub tool_call_id: Option<String>,
}
```

### `ToolSchema`

```rust
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}
```

### `ToolResult`

```rust
pub struct ToolResult {
    pub output: String,
    pub success: bool,
    pub duration_ms: u64,
}
```

### `MemoryRecord`

```rust
pub struct MemoryRecord {
    pub id: String,
    pub text: String,
    pub importance: f32,
    pub tags: Vec<String>,
    pub metadata: HashMap<String, String>,
    pub created_at: String,
}
```

### `Goal`

```rust
pub struct Goal {
    pub id: String,
    pub description: String,
    pub priority: u8,
    pub created_at: String,
    pub status: GoalStatus,
}
```

### `Plan`

```rust
pub struct Plan {
    pub id: String,
    pub goal_id: String,
    pub steps: Vec<PlanStep>,
}
```

## Enums

### `ChatRole`

```rust
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}
```

### `PermissionLevel`

```rust
pub enum PermissionLevel {
    Read,
    Write,
    Destructive,
}
```

### `GoalStatus`

```rust
pub enum GoalStatus {
    Active,
    InProgress,
    Completed,
    Failed,
}
```
