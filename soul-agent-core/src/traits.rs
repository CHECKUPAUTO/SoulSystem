//! # Framework Public API — Traits
//!
//! These traits define the stable public API for the SoulSystem framework.
//! External crates implement these traits to provide custom LLM providers,
//! memory backends, tools, and planners.
//!
//! ## Example
//!
//! ```ignore
//! use soul_agent_core::traits::{LLMClient, ChatSession, ChatMessage};
//!
//! struct MyCustomLLM { /* ... */ }
//!
//! #[async_trait::async_trait]
//! impl LLMClient for MyCustomLLM {
//!     async fn chat(&self, session: &mut ChatSession, tools: Option<&[ToolSchema]>) -> Result<String, String> {
//!         // Custom implementation
//!         Ok("response".into())
//!     }
//! }
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── LLM Provider Traits ──────────────────────────────────────────────

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Tool call requested by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallInfo {
    pub id: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

/// Schema describing a tool available to the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Conversation session with message history and context management.
pub struct ChatSessionBuilder {
    pub messages: Vec<ChatMessage>,
    pub max_context_chars: usize,
}

impl ChatSessionBuilder {
    pub fn new(system_prompt: &str, max_context_chars: usize) -> Self {
        let mut messages = Vec::new();
        messages.push(ChatMessage {
            role: ChatRole::System,
            content: system_prompt.to_string(),
            tool_calls: None,
            tool_call_id: None,
        });
        Self {
            messages,
            max_context_chars,
        }
    }

    pub fn add_user_message(&mut self, content: &str) {
        self.messages.push(ChatMessage {
            role: ChatRole::User,
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
        });
    }

    pub fn add_assistant_message(&mut self, content: &str) {
        self.messages.push(ChatMessage {
            role: ChatRole::Assistant,
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
        });
    }

    pub fn add_tool_result(&mut self, tool_call_id: &str, result: &str) {
        self.messages.push(ChatMessage {
            role: ChatRole::Tool,
            content: result.to_string(),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.to_string()),
        });
    }

    pub fn build_messages(&self) -> Vec<ChatMessage> {
        self.messages.clone()
    }

    pub fn clear(&mut self) {
        let system: Vec<ChatMessage> = self
            .messages
            .iter()
            .filter(|m| m.role == ChatRole::System)
            .cloned()
            .collect();
        self.messages = system;
    }
}

/// Multi-provider LLM client trait.
///
/// Implement this trait to create a custom LLM provider (OpenAI, Anthropic, local, etc.).
/// The framework calls `chat()` in a loop during the ReAct cycle.
#[async_trait]
pub trait LLMClient: Send + Sync {
    /// Send messages to the LLM and return the assistant's response.
    ///
    /// If `tools` is provided, the model should return tool calls when appropriate.
    /// The response content should be the assistant's text reply.
    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolSchema]>,
    ) -> Result<String, LLMError>;

    /// Send a single prompt for generation (no tool calling).
    async fn generate(&self, prompt: &str) -> Result<String, LLMError>;

    /// Return the model name/identifier.
    fn model_name(&self) -> &str;
}

#[derive(Debug, Clone)]
pub enum LLMError {
    Network(String),
    Timeout(String),
    RateLimited(String),
    InvalidRequest(String),
    Unknown(String),
}

impl std::fmt::Display for LLMError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LLMError::Network(e) => write!(f, "network error: {}", e),
            LLMError::Timeout(e) => write!(f, "timeout: {}", e),
            LLMError::RateLimited(e) => write!(f, "rate limited: {}", e),
            LLMError::InvalidRequest(e) => write!(f, "invalid request: {}", e),
            LLMError::Unknown(e) => write!(f, "unknown error: {}", e),
        }
    }
}

impl std::error::Error for LLMError {}

// ── Memory Traits ────────────────────────────────────────────────────

/// Memory entry stored in the memory backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: String,
    pub text: String,
    pub importance: f32,
    pub tags: Vec<String>,
    pub metadata: HashMap<String, String>,
    pub created_at: String,
}

/// Result from a memory search query.
#[derive(Debug, Clone)]
pub struct MemorySearchResult {
    pub record: MemoryRecord,
    pub score: f32,
}

/// Hierarchical memory interface.
///
/// Implement this trait to provide a custom memory backend (Qdrant, Redis, in-memory, etc.).
/// The framework uses memory to:
/// - Store distilled learnings from each task
/// - Retrieve relevant past experiences for context
/// - Maintain working/episodic/semantic memory layers
#[async_trait]
pub trait Memory: Send + Sync {
    /// Store a new memory record.
    async fn store(&self, record: MemoryRecord) -> Result<(), MemoryError>;

    /// Search for relevant memories by query string.
    ///
    /// Returns up to `k` results sorted by relevance score.
    async fn search(&self, query: &str, k: usize) -> Result<Vec<MemorySearchResult>, MemoryError>;

    /// Get context string for a query (formatted for prompt injection).
    async fn get_context(&self, query: &str) -> Result<String, MemoryError>;

    /// Decay old memories and prune below threshold.
    ///
    /// Returns (kept_count, removed_count).
    fn decay_and_prune(
        &self,
        decay_factor: f32,
        threshold: f32,
        max_entries: usize,
    ) -> Result<(usize, usize), MemoryError>;

    /// Total number of stored memories.
    fn count(&self) -> usize;
}

#[derive(Debug, Clone)]
pub enum MemoryError {
    StorageError(String),
    SerializationError(String),
    NotFound(String),
}

impl std::fmt::Display for MemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryError::StorageError(e) => write!(f, "storage error: {}", e),
            MemoryError::SerializationError(e) => write!(f, "serialization error: {}", e),
            MemoryError::NotFound(e) => write!(f, "not found: {}", e),
        }
    }
}

impl std::error::Error for MemoryError {}

// ── Tool Traits ──────────────────────────────────────────────────────

/// Tool execution result.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub output: String,
    pub success: bool,
    pub duration_ms: u64,
}

/// Permission level required to execute a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionLevel {
    Read,
    Write,
    Destructive,
}

/// Tool interface for the framework.
///
/// Implement this trait to create custom tools (APIs, databases, etc.).
/// The framework calls `execute()` during the ReAct loop when the model
/// requests a tool call.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Return the tool schema describing name, description, and parameters.
    fn schema(&self) -> ToolSchema;

    /// Execute the tool with the given arguments.
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError>;

    /// Return the permission level required to run this tool.
    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Read
    }
}

#[derive(Debug, Clone)]
pub enum ToolError {
    ExecutionError(String),
    InvalidArguments(String),
    PermissionDenied(String),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::ExecutionError(e) => write!(f, "execution error: {}", e),
            ToolError::InvalidArguments(e) => write!(f, "invalid arguments: {}", e),
            ToolError::PermissionDenied(e) => write!(f, "permission denied: {}", e),
        }
    }
}

impl std::error::Error for ToolError {}

// ── Planner Traits ───────────────────────────────────────────────────

/// Goal for the planner to decompose.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub description: String,
    pub priority: u8,
    pub created_at: String,
    pub status: GoalStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GoalStatus {
    Active,
    InProgress,
    Completed,
    Failed,
}

/// A step in a plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub command: String,
    pub description: String,
    pub order: usize,
}

/// A decomposed plan with ordered steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub goal_id: String,
    pub steps: Vec<PlanStep>,
}

/// Planner interface for goal decomposition.
///
/// Implement this trait to provide custom planning logic (LLM-based, rule-based, etc.).
/// The framework calls `create_plan()` at the start of each task,
/// and `decide()` after each step to decide the next action.
#[async_trait]
pub trait Planner: Send + Sync {
    /// Create a plan from a goal description.
    async fn create_plan(&self, goal: &Goal) -> Result<Plan, PlannerError>;

    /// Decide the next action based on current context.
    async fn decide(&self, context: &str) -> Result<String, PlannerError>;

    /// Evaluate the outcome of a plan step.
    fn evaluate(&self, step: &PlanStep, outcome: &str) -> f32;
}

#[derive(Debug, Clone)]
pub enum PlannerError {
    PlanningFailed(String),
    DecisionFailed(String),
    LLMError(String),
}

impl std::fmt::Display for PlannerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlannerError::PlanningFailed(e) => write!(f, "planning failed: {}", e),
            PlannerError::DecisionFailed(e) => write!(f, "decision failed: {}", e),
            PlannerError::LLMError(e) => write!(f, "LLM error: {}", e),
        }
    }
}

impl std::error::Error for PlannerError {}

// ── Agent Trait ──────────────────────────────────────────────────────

/// Core agent interface.
///
/// This trait defines the high-level agent API. Implement it for custom agent behaviors.
/// The framework provides a default implementation in [`crate::AutonomousAgent`].
#[async_trait]
pub trait Agent: Send + Sync {
    /// Execute a task using the ReAct loop (observe → think → act → evaluate).
    async fn run_task(&mut self, task: &str) -> Result<String, AgentError>;

    /// Ask a question and get a response (conversation mode).
    async fn ask(&mut self, question: &str) -> Result<String, AgentError>;

    /// Abort the current running task.
    async fn abort(&self);

    /// Get agent status as JSON.
    fn status(&self) -> serde_json::Value;
}

#[derive(Debug, Clone)]
pub enum AgentError {
    TaskFailed(String),
    LLMError(String),
    ToolError(String),
    Aborted,
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentError::TaskFailed(e) => write!(f, "task failed: {}", e),
            AgentError::LLMError(e) => write!(f, "LLM error: {}", e),
            AgentError::ToolError(e) => write!(f, "tool error: {}", e),
            AgentError::Aborted => write!(f, "aborted"),
        }
    }
}

impl std::error::Error for AgentError {}
