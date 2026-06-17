//! # Agent Builder
//!
//! Builder pattern for composing agents from LLM, memory, tools, and planner components.
//!
//! ## Example
//!
//! ```ignore
//! use soul_agent_core::builder::AgentBuilder;
//! use soul_agent_core::traits::*;
//!
//! let agent = AgentBuilder::new()
//!     .llm(my_llm_client)
//!     .memory(my_memory)
//!     .tool(my_custom_tool)
//!     .tool(another_tool)
//!     .planner(my_planner)
//!     .build();
//!
//! let result = agent.run_task("Explore the /tmp directory").await;
//! ```

use crate::traits::*;
use std::collections::HashMap;
use std::sync::Arc;

/// Builder for composing agents from trait objects.
///
/// Use this to create agents with custom LLM, memory, tools, and planner.
/// All components are optional — defaults are used when not provided.
pub struct AgentBuilder<L: LLMClient, M: Memory, T: Tool, P: Planner> {
    llm: Option<Arc<L>>,
    memory: Option<Arc<M>>,
    tools: Vec<Arc<T>>,
    planner: Option<Arc<P>>,
    config: AgentConfig,
}

/// Agent configuration.
#[derive(Debug, Clone)]
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

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: "SoulSystem".to_string(),
            max_turns: 50,
            max_tool_retries: 3,
            shell_timeout_secs: 60,
            safety_warning_turns: vec![7, 10, 15, 25, 35, 50],
            auto_distill: true,
            working_memory_capacity: 50,
            max_context_chars: 40000,
        }
    }
}

impl<L: LLMClient, M: Memory, T: Tool, P: Planner> AgentBuilder<L, M, T, P> {
    /// Create a new builder with default configuration.
    pub fn new() -> Self {
        Self {
            llm: None,
            memory: None,
            tools: Vec::new(),
            planner: None,
            config: AgentConfig::default(),
        }
    }

    /// Set the LLM client.
    pub fn llm(mut self, client: L) -> Self {
        self.llm = Some(Arc::new(client));
        self
    }

    /// Set the memory backend.
    pub fn memory(mut self, mem: M) -> Self {
        self.memory = Some(Arc::new(mem));
        self
    }

    /// Add a tool to the agent.
    pub fn tool(mut self, t: T) -> Self {
        self.tools.push(Arc::new(t));
        self
    }

    /// Set the planner.
    pub fn planner(mut self, p: P) -> Self {
        self.planner = Some(Arc::new(p));
        self
    }

    /// Set agent configuration.
    pub fn config(mut self, cfg: AgentConfig) -> Self {
        self.config = cfg;
        self
    }

    /// Set agent name.
    pub fn name(mut self, name: &str) -> Self {
        self.config.name = name.to_string();
        self
    }

    /// Set max turns for the ReAct loop.
    pub fn max_turns(mut self, n: usize) -> Self {
        self.config.max_turns = n;
        self
    }

    /// Set max context characters for the chat session.
    pub fn max_context_chars(mut self, n: usize) -> Self {
        self.config.max_context_chars = n;
        self
    }

    /// Build the agent from the provided components.
    ///
    /// Returns a `ComposedAgent` that implements the `Agent` trait.
    pub fn build(self) -> Result<ComposedAgent<L, M, T, P>, BuilderError> {
        let llm = self.llm.ok_or(BuilderError::MissingLLM)?;
        let name = self.config.name.clone();
        let max_context = self.config.max_context_chars;

        Ok(ComposedAgent {
            llm,
            memory: self.memory,
            tools: self.tools,
            planner: self.planner,
            config: self.config,
            chat_session: ChatSessionBuilder::new(
                &build_system_prompt(&name),
                max_context,
            ),
            turn: 0,
            consecutive_failures: 0,
            running: Arc::new(tokio::sync::RwLock::new(false)),
        })
    }

    /// Build without requiring all components (uses defaults).
    ///
    /// Requires at least an LLM client. Other components use defaults.
    pub fn build_with_defaults(self) -> Result<ComposedAgent<L, M, T, P>, BuilderError> {
        self.build()
    }
}

impl<L: LLMClient, M: Memory, T: Tool, P: Planner> Default for AgentBuilder<L, M, T, P> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub enum BuilderError {
    MissingLLM,
    MissingMemory,
    MissingPlanner,
}

impl std::fmt::Display for BuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuilderError::MissingLLM => write!(f, "LLM client is required"),
            BuilderError::MissingMemory => write!(f, "Memory backend is required"),
            BuilderError::MissingPlanner => write!(f, "Planner is required"),
        }
    }
}

impl std::error::Error for BuilderError {}

// ── Composed Agent ───────────────────────────────────────────────────

/// Agent composed from trait objects via [`AgentBuilder`].
///
/// This is the framework's default agent implementation. It uses the ReAct loop
/// and delegates to the provided LLM, memory, tools, and planner.
pub struct ComposedAgent<L: LLMClient, M: Memory, T: Tool, P: Planner> {
    llm: Arc<L>,
    memory: Option<Arc<M>>,
    tools: Vec<Arc<T>>,
    planner: Option<Arc<P>>,
    config: AgentConfig,
    chat_session: ChatSessionBuilder,
    turn: usize,
    consecutive_failures: usize,
    running: Arc<tokio::sync::RwLock<bool>>,
}

impl<L: LLMClient, M: Memory, T: Tool, P: Planner> ComposedAgent<L, M, T, P> {
    fn tool_schemas(&self) -> Vec<ToolSchema> {
        self.tools.iter().map(|t| t.schema()).collect()
    }

    fn tool_by_name(&self, name: &str) -> Option<&Arc<T>> {
        self.tools.iter().find(|t| t.schema().name == name)
    }
}

#[async_trait::async_trait]
impl<L: LLMClient, M: Memory, T: Tool, P: Planner> Agent for ComposedAgent<L, M, T, P> {
    async fn run_task(&mut self, task: &str) -> Result<String, AgentError> {
        *self.running.write().await = true;
        self.turn = 0;
        self.chat_session.clear();
        self.chat_session.add_user_message(task);

        let schemas = self.tool_schemas();
        let schemas_ref = if schemas.is_empty() { None } else { Some(schemas.as_slice()) };

        let mut last_response = String::new();

        while self.turn < self.config.max_turns {
            if !*self.running.read().await {
                return Err(AgentError::Aborted);
            }

            self.turn += 1;

            // Safety warnings
            if self.config.safety_warning_turns.contains(&self.turn) {
                let warning = format!(
                    "SAFETY: You have been running for {} turns. If stuck, change strategy or ask for help.",
                    self.turn
                );
                self.chat_session.add_user_message(&warning);
            }

            // Inject memory context
            if let Some(ref memory) = self.memory {
                if let Ok(ctx) = memory.get_context(task).await {
                    if !ctx.is_empty() {
                        self.chat_session.add_user_message(&format!(
                            "Relevant memories:\n{}",
                            ctx
                        ));
                    }
                }
            }

            // Call LLM
            let messages = self.chat_session.build_messages();
            let response = self.llm.chat(&messages, schemas_ref).await.map_err(|e| {
                self.consecutive_failures += 1;
                AgentError::LLMError(e.to_string())
            })?;

            // Check for tool calls (response contains tool call JSON markers)
            let content = response.clone();

            // Simple heuristic: if response contains a tool call pattern, try to execute
            if let Some(tool_result) = try_extract_and_execute_tool(&content, &self.tools).await {
                let tool_name = tool_result.0;
                let tool_output = tool_result.1;
                let tool_success = tool_result.2;

                self.chat_session.add_tool_result(&tool_name, &tool_output);

                if tool_success {
                    self.consecutive_failures = 0;
                } else {
                    self.consecutive_failures += 1;
                }

                last_response = tool_output;
                continue;
            }

            // No tool calls — response is the final answer
            if !content.is_empty() {
                last_response = content.clone();
                self.chat_session.add_assistant_message(&content);
            }

            // Check completion signals
            let lower = last_response.to_lowercase();
            if lower.contains("task completed")
                || lower.contains("done")
                || lower.contains("finished")
                || lower.contains("completed successfully")
            {
                break;
            }

            if content.is_empty() {
                break;
            }
        }

        *self.running.write().await = false;

        Ok(last_response)
    }

    async fn ask(&mut self, question: &str) -> Result<String, AgentError> {
        self.chat_session.add_user_message(question);

        let messages = self.chat_session.build_messages();
        let schemas = self.tool_schemas();
        let schemas_ref = if schemas.is_empty() { None } else { Some(schemas.as_slice()) };

        let response = self.llm.chat(&messages, schemas_ref).await.map_err(|e| {
            AgentError::LLMError(e.to_string())
        })?;

        let content = response;
        self.chat_session.add_assistant_message(&content);
        Ok(content)
    }

    async fn abort(&self) {
        *self.running.write().await = false;
    }

    fn status(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.config.name,
            "turn": self.turn,
            "max_turns": self.config.max_turns,
            "tools": self.tools.len(),
            "consecutive_failures": self.consecutive_failures,
            "has_memory": self.memory.is_some(),
            "has_planner": self.planner.is_some(),
        })
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

fn build_system_prompt(name: &str) -> String {
    format!(
        r#"You are {name}, an autonomous AI agent running on a Linux server.

CAPABILITIES:
- Execute shell commands via execute_shell
- Read, write, patch files via read_file, write_file, patch_file
- List directories via list_directory
- Search files via search_files
- Search file contents via grep_content

BEHAVIOR:
- Think step by step before acting
- Use tools to gather information before making changes
- Verify results after each action
- If a tool call fails, analyze the error and try a different approach
- When the task is complete, say "Task completed" clearly
- Never execute destructive commands (rm -rf, mkfs, etc.) without explicit user approval
- Keep responses concise

You have access to tool calling. Use it whenever you need to interact with the system."#,
        name = name,
    )
}

/// Try to extract a tool call from the response and execute it.
///
/// Returns (tool_name, output, success) or None if no tool call found.
async fn try_extract_and_execute_tool<T: Tool>(
    content: &str,
    tools: &[Arc<T>],
) -> Option<(String, String, bool)> {
    // Look for JSON tool call patterns
    // Format: {"tool": "name", "args": {...}} or similar
    let lower = content.to_lowercase();

    for tool in tools {
        let schema = tool.schema();
        // Check if the tool name is mentioned in the response
        if lower.contains(&schema.name.to_lowercase()) {
            // Try to execute with empty args (simplified)
            match tool.execute(serde_json::json!({})).await {
                Ok(result) => {
                    return Some((schema.name, result.output, result.success));
                }
                Err(e) => {
                    return Some((schema.name, e.to_string(), false));
                }
            }
        }
    }

    None
}

// ── Plugin Registry ─────────────────────────────────────────────────

/// Plugin registry for dynamic tool and memory loading.
///
/// Use this to register tools and memory backends at runtime.
pub struct PluginRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
    memory_backends: HashMap<String, Box<dyn Memory>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            memory_backends: HashMap::new(),
        }
    }

    /// Register a tool with the registry.
    pub fn register_tool<T: Tool + 'static>(&mut self, name: &str, tool: T) {
        self.tools.insert(name.to_string(), Box::new(tool));
    }

    /// Get a tool by name.
    pub fn get_tool(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// List all registered tool names.
    pub fn list_tools(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    /// Register a memory backend with the registry.
    pub fn register_memory<M: Memory + 'static>(&mut self, name: &str, memory: M) {
        self.memory_backends
            .insert(name.to_string(), Box::new(memory));
    }

    /// Get a memory backend by name.
    pub fn get_memory(&self, name: &str) -> Option<&dyn Memory> {
        self.memory_backends.get(name).map(|m| m.as_ref())
    }

    /// List all registered memory backend names.
    pub fn list_memory_backends(&self) -> Vec<&str> {
        self.memory_backends.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}
