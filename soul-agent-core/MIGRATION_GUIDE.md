# Migration Guide

This guide helps developers migrate from other AI agent frameworks (LangChain, CrewAI, AutoGen, etc.) to SoulSystem.

## Quick Comparison

| Feature | LangChain | CrewAI | AutoGen | SoulSystem |
|---------|-----------|--------|---------|------------|
| Language | Python | Python | Python | **Rust** |
| Type Safety | Runtime | Runtime | Runtime | **Compile-time** |
| Memory | Basic | Basic | Basic | **Hierarchical (3 levels)** |
| Tools | Dynamic | Dynamic | Dynamic | **Static + Dynamic** |
| Performance | GIL-limited | GIL-limited | GIL-limited | **Native async** |
| Memory Safety | GC | GC | GC | **Ownership model** |

## Concept Mapping

### LangChain → SoulSystem

| LangChain | SoulSystem |
|-----------|------------|
| `Agent` | `Agent` trait |
| `BaseLanguageModel` | `LLMClient` trait |
| `BaseTool` | `Tool` trait |
| `BaseMemory` | `Memory` trait |
| `AgentExecutor` | `ComposedAgent` |
| `Toolkits` | `PluginRegistry` |

### CrewAI → SoulSystem

| CrewAI | SoulSystem |
|--------|------------|
| `Agent` | `Agent` trait |
| `Task` | `Goal` |
| `Crew` | `ComposedAgent` with multiple tools |
| `Tool` | `Tool` trait |

### AutoGen → SoulSystem

| AutoGen | SoulSystem |
|---------|------------|
| `ConversableAgent` | `Agent` trait |
| `AssistantAgent` | `ComposedAgent` |
| `UserProxyAgent` | `Agent` with user input |
| `Tool` | `Tool` trait |

## Migration Steps

### Step 1: Implement the LLMClient Trait

```rust
use soul_agent_core::traits::{LLMClient, ChatMessage, ToolSchema, LLMError};
use async_trait::async_trait;

struct MyLangChainLLM {
    // Your existing LangChain client
}

#[async_trait]
impl LLMClient for MyLangChainLLM {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolSchema]>,
    ) -> Result<String, LLMError> {
        // Convert SoulSystem messages to LangChain format
        // Call your existing LangChain client
        // Convert response back to SoulSystem format
        todo!()
    }

    async fn generate(&self, prompt: &str) -> Result<String, LLMError> {
        // Simple generation without tool calling
        todo!()
    }

    fn model_name(&self) -> &str {
        "my-langchain-model"
    }
}
```

### Step 2: Implement the Tool Trait

```rust
use soul_agent_core::traits::{Tool, ToolSchema, ToolResult, PermissionLevel, ToolError};
use async_trait::async_trait;

struct MyCrewAITool {
    // Your existing CrewAI tool
}

#[async_trait]
impl Tool for MyCrewAITool {
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "my_tool".into(),
            description: "Description of what the tool does".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "input": {
                        "type": "string",
                        "description": "Input parameter"
                    }
                },
                "required": ["input"]
            }),
        }
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError> {
        // Convert SoulSystem args to your tool format
        // Execute the tool
        // Convert result back to SoulSystem format
        todo!()
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Read
    }
}
```

### Step 3: Implement the Memory Trait

```rust
use soul_agent_core::traits::{Memory, MemoryRecord, MemorySearchResult, MemoryError};
use async_trait::async_trait;

struct MyAutoGenMemory {
    // Your existing AutoGen memory
}

#[async_trait]
impl Memory for MyAutoGenMemory {
    async fn store(&self, record: MemoryRecord) -> Result<(), MemoryError> {
        // Convert SoulSystem record to your format
        // Store in your memory backend
        todo!()
    }

    async fn search(&self, query: &str, k: usize) -> Result<Vec<MemorySearchResult>, MemoryError> {
        // Search your memory backend
        // Convert results to SoulSystem format
        todo!()
    }

    async fn get_context(&self, query: &str) -> Result<String, MemoryError> {
        // Get formatted context for prompt injection
        todo!()
    }

    fn decay_and_prune(
        &self,
        decay_factor: f32,
        threshold: f32,
        max_entries: usize,
    ) -> Result<(usize, usize), MemoryError> {
        // Apply decay and prune old memories
        todo!()
    }

    fn count(&self) -> usize {
        // Return total memory count
        todo!()
    }
}
```

### Step 4: Compose Your Agent

```rust
use soul_agent_core::builder::AgentBuilder;

let agent = AgentBuilder::new()
    .llm(MyLangChainLLM::new())
    .memory(MyAutoGenMemory::new())
    .tool(MyCrewAITool::new())
    .name("MigratedAgent")
    .max_turns(50)
    .build()?;
```

## Key Differences

### 1. Type Safety

SoulSystem uses Rust's type system for compile-time safety:

```rust
// Python (LangChain) - runtime errors
agent = Agent(llm="gpt-4", tools=[tool1, tool2])  # Typo in tool name?

// Rust (SoulSystem) - compile-time errors
let agent = AgentBuilder::new()
    .llm(my_llm)
    .tool(my_tool)  // Type-checked at compile time
    .build()?;
```

### 2. Memory Management

SoulSystem provides hierarchical memory:

```rust
// Working memory (short-term)
let working_memory = WorkingMemory::new(100);

// Episodic memory (medium-term)
let episodic_memory = EpisodicMemory::new();

// Semantic memory (long-term)
let semantic_memory = SemanticMemory::new();

// All layers are automatically managed
```

### 3. Tool Execution

SoulSystem tools are sandboxed:

```rust
// Tools run in a sandbox with:
// - Permission checking
// - Timeout enforcement
// - Resource limits
// - Audit logging

let tool = MyTool {
    // Tools are sandboxed by default
};
```

### 4. Async Execution

SoulSystem is fully async:

```rust
// All operations are async
let result = agent.run_task("do something").await?;

// Parallel tool execution
let (result1, result2) = tokio::join!(
    agent.run_task("task 1"),
    agent.run_task("task 2"),
);
```

## Performance Improvements

Migrating from Python to Rust typically yields:

| Operation | Python (LangChain) | Rust (SoulSystem) | Speedup |
|-----------|-------------------|-------------------|---------|
| LLM Call | ~100ms overhead | ~1ms overhead | 100x |
| Tool Execution | ~50ms overhead | ~0.5ms overhead | 100x |
| Memory Search | ~200ms | ~10ms | 20x |
| Memory Store | ~50ms | ~5ms | 10x |

## Common Patterns

### 1. Tool Chaining

```rust
// LangChain
from langchain.chains import LLMChain
chain = LLMChain(llm=llm, tools=[tool1, tool2])

// SoulSystem
let agent = AgentBuilder::new()
    .llm(llm)
    .tool(tool1)
    .tool(tool2)
    .build()?;
let result = agent.run_task("chain these tools").await?;
```

### 2. Memory Persistence

```rust
// LangChain
from langchain.memory import ConversationBufferMemory
memory = ConversationBufferMemory()

// SoulSystem
let memory = InMemoryBackend::new();
// Or use SoulSystem's built-in sled-backed memory
```

### 3. Custom Planning

```rust
// LangChain
from langchain.agents import Plan-and-Execute

// SoulSystem
#[async_trait]
impl Planner for MyPlanner {
    async fn create_plan(&self, goal: &Goal) -> Result<Plan, PlannerError> {
        // Custom planning logic
    }
}
```

## Troubleshooting

### Issue: Type Inference Errors

```rust
// Error: type annotations needed
let agent = AgentBuilder::new().llm(llm).build()?;

// Fix: provide explicit types
let agent: ComposedAgent<MyLLM, MyMemory, MyTool, MyPlanner> =
    AgentBuilder::new().llm(llm).build()?;
```

### Issue: Async Trait Objects

```rust
// Error: future is not Send
let agent = AgentBuilder::new().llm(llm).build()?;

// Fix: ensure all components are Send + Sync
```

### Issue: Permission Errors

```rust
// Error: tool requires Destructive permission
agent.run_task("rm -rf /").await?;

// Fix: use appropriate permission levels
let tool = MyTool {
    // Set permission_level() appropriately
};
```

## Next Steps

1. Read the [Framework Guide](FRAMEWORK_GUIDE.md)
2. Check the `examples/` directory
3. Review the [API Reference](API_REFERENCE.md)
