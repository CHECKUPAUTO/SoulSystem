//! SoulSystem Autonomous Agent Core
//!
//! Implements the ReAct (Reason + Act) loop with:
//! - Conversation context management
//! - Tool dispatch with before/after hooks
//! - Working memory checkpoints
//! - Safety warnings and turn limits
//! - Task queue with abort support
//! - Memory distillation

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use soul_llm::{ChatSession, OllamaClient, ToolCall, ToolSchema, build_tool_schemas};
use soul_planner::{CognitiveLoop, Goal, GoalStatus, WorkingMemory};
use soul_tools::{AsyncShellExecutor, async_dispatch_tool, dispatch_tool, discover_system_tools, ToolRegistry};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};
use uuid::Uuid;

// ── Agent Configuration ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub name: String,
    pub max_turns: usize,
    pub max_tool_retries: usize,
    pub shell_timeout_secs: u64,
    pub safety_warning_turns: Vec<usize>,
    pub auto_distill: bool,
    pub auto_repair: bool,
    pub max_consecutive_failures: usize,
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
            auto_repair: true,
            max_consecutive_failures: 3,
        }
    }
}

// ── Step Outcome ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum StepOutcome {
    Continue { next_prompt: Option<String> },
    Done { result: String },
    Interrupt { question: String, candidates: Vec<String> },
    Error { message: String },
}

// ── Agent Event (for streaming) ──────────────────────────────────────

#[derive(Debug, Clone)]
pub enum AgentEvent {
    Thinking { content: String },
    ToolCall { name: String, args: serde_json::Value },
    ToolResult { name: String, output: String, success: bool },
    Response { content: String },
    SafetyWarning { message: String },
    Done { summary: String },
    Error { message: String },
}

// ── Autonomous Agent ─────────────────────────────────────────────────

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
    event_tx: Option<mpsc::UnboundedSender<AgentEvent>>,
}

impl AutonomousAgent {
    pub fn new(llm: OllamaClient, config: AgentConfig) -> Self {
        let system_prompt = build_system_prompt(&config.name);
        let chat_session = ChatSession::with_max_context(&system_prompt, 40000);

        let tools = discover_system_tools();
        let mut registry = ToolRegistry::new();
        for tool in tools {
            registry.register(tool);
        }

        let tool_schemas = build_tool_schemas();

        Self {
            config: config.clone(),
            llm,
            chat_session,
            planner: CognitiveLoop::new(),
            registry,
            executor: AsyncShellExecutor::new(config.shell_timeout_secs),
            tool_schemas,
            history: Vec::new(),
            turn: 0,
            consecutive_failures: 0,
            repair_count: 0,
            running: Arc::new(RwLock::new(false)),
            event_tx: None,
        }
    }

    pub fn set_event_sender(&mut self, tx: mpsc::UnboundedSender<AgentEvent>) {
        self.event_tx = Some(tx);
    }

    fn emit_event(&self, event: AgentEvent) {
        if let Some(tx) = &self.event_tx {
            let _ = tx.send(event);
        }
    }

    // ── Core ReAct Loop ──

    pub async fn run_task(&mut self, task: &str) -> Result<String, String> {
        *self.running.write().await = true;
        self.turn = 0;
        self.chat_session.clear();

        // Set initial working memory
        self.planner.memory.set_key_info(task);

        self.chat_session.add_user_message(task);

        let mut last_response = String::new();

        while self.turn < self.config.max_turns {
            if !*self.running.read().await {
                return Err("Task aborted".to_string());
            }

            self.turn += 1;

            // Safety warnings
            if self.config.safety_warning_turns.contains(&self.turn) {
                let warning = format!(
                    "SAFETY: You have been running for {} turns. If stuck, change strategy or ask for help.",
                    self.turn
                );
                self.emit_event(AgentEvent::SafetyWarning {
                    message: warning.clone(),
                });
                self.chat_session.add_user_message(&warning);
            }

            // Inject working memory context
            let memory_context = self.planner.memory.to_prompt_section();
            if !memory_context.is_empty() && self.turn % 5 == 1 {
                self.chat_session.add_user_message(&memory_context);
            }

            // Auto-compact context before each LLM call
            self.compact_if_needed();

            // Build messages and call LLM
            let messages = self.chat_session.build_messages();

            self.emit_event(AgentEvent::Thinking {
                content: format!("Turn {}/{}", self.turn, self.config.max_turns),
            });

            let response = match self
                .llm
                .chat(&messages, Some(&self.tool_schemas))
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    self.consecutive_failures += 1;
                    let repairs = self.auto_repair();
                    if !repairs.is_empty() {
                        for r in &repairs {
                            self.emit_event(AgentEvent::SafetyWarning {
                                message: r.clone(),
                            });
                        }
                    }
                    return Err(format!("LLM error: {}", e));
                }
            };

            // Process response
            let msg = &response.message;
            let content = msg.content.clone().unwrap_or_default();

            if let Some(tool_calls) = &msg.tool_calls {
                if !tool_calls.is_empty() {
                    // Assistant made tool calls
                    self.chat_session.add_assistant_with_tools(
                        if content.is_empty() { None } else { Some(&content) },
                        tool_calls.clone(),
                    );

                    // Execute each tool call
                    for tc in tool_calls {
                        let name = tc.function.name.clone();
                        let args: serde_json::Value =
                            serde_json::from_str(&tc.function.arguments)
                                .unwrap_or(serde_json::json!({}));

                        self.emit_event(AgentEvent::ToolCall {
                            name: name.clone(),
                            args: args.clone(),
                        });

                        // Permission check
                        let permission = if name == "execute_shell" {
                            let cmd = args.get("command").and_then(|c| c.as_str()).unwrap_or("");
                            soul_tools::PermissionLevel::from_command(cmd)
                        } else {
                            soul_tools::PermissionLevel::Read
                        };

                        if permission == soul_tools::PermissionLevel::Destructive {
                            let msg = "BLOCKED: Destructive command detected. This requires explicit confirmation.";
                            self.emit_event(AgentEvent::ToolResult {
                                name: name.clone(),
                                output: msg.to_string(),
                                success: false,
                            });
                            self.chat_session.add_tool_result(&tc.id, msg);
                            continue;
                        }

                        // Audit log for Write-level commands
                        if permission == soul_tools::PermissionLevel::Write {
                            let audit_msg = format!(
                                "AUDIT: Write-level command executed: {}({})",
                                name,
                                truncate_output(&args.to_string(), 100)
                            );
                            tracing::warn!("{}", audit_msg);
                            self.emit_event(AgentEvent::SafetyWarning {
                                message: audit_msg,
                            });
                        }

                        // Execute tool
                        let result = match async_dispatch_tool(&name, args.clone()).await {
                            Ok(output) => {
                                self.consecutive_failures = 0;
                                self.emit_event(AgentEvent::ToolResult {
                                    name: name.clone(),
                                    output: truncate_output(&output, 2000),
                                    success: true,
                                });
                                output
                            }
                            Err(e) => {
                                self.consecutive_failures += 1;
                                self.emit_event(AgentEvent::ToolResult {
                                    name: name.clone(),
                                    output: e.clone(),
                                    success: false,
                                });
                                let repairs = self.auto_repair();
                                if !repairs.is_empty() {
                                    for r in &repairs {
                                        self.emit_event(AgentEvent::SafetyWarning {
                                            message: r.clone(),
                                        });
                                    }
                                }
                                e
                            }
                        };

                        self.planner.history.record(
                            format!("{}({})", name, truncate_output(&args.to_string(), 100)),
                            truncate_output(&result, 200),
                            true,
                        );

                        self.chat_session.add_tool_result(&tc.id, &truncate_output(&result, 3000));
                    }

                    continue;
                }
            }

            // No tool calls — response is the final answer
            if !content.is_empty() {
                last_response = content.clone();
                self.chat_session.add_assistant_message(&content);
                self.emit_event(AgentEvent::Response {
                    content: content.clone(),
                });
            }

            // Check if the response indicates completion
            let lower = last_response.to_lowercase();
            if lower.contains("task completed")
                || lower.contains("done")
                || lower.contains("finished")
                || lower.contains("completed successfully")
            {
                break;
            }

            // If no content and no tools, the LLM is done
            if content.is_empty() && msg.tool_calls.is_none() {
                break;
            }
        }

        *self.running.write().await = false;

        // Distill learnings
        if self.config.auto_distill && !last_response.is_empty() {
            self.distill_memory(task, &last_response).await;
        }

        // Self-critique: evaluate output quality after task completion
        if !last_response.is_empty() {
            let critique = soul_critique::quick_critique(task, &last_response);
            if !critique.passed {
                tracing::warn!(
                    "Self-critique: {:.1}/10 — {}",
                    critique.overall_score,
                    critique.feedback.lines().next().unwrap_or("")
                );
                self.emit_event(AgentEvent::SafetyWarning {
                    message: format!(
                        "Quality: {:.1}/10 — review recommended",
                        critique.overall_score
                    ),
                });
            } else {
                tracing::info!("Self-critique: {:.1}/10 PASS", critique.overall_score);
            }
        }

        let summary = format!(
            "Task completed in {} turns. Result: {}",
            self.turn,
            truncate_output(&last_response, 500)
        );

        self.emit_event(AgentEvent::Done {
            summary: summary.clone(),
        });

        Ok(last_response)
    }

    // ── Interactive Ask ──

    pub async fn ask(&mut self, question: &str) -> Result<String, String> {
        self.chat_session.add_user_message(question);

        // Auto-compact before building messages if context is large
        self.compact_if_needed();

        let messages = self.chat_session.build_messages();

        let response = self
            .llm
            .chat(&messages, Some(&self.tool_schemas))
            .await
            .map_err(|e| format!("LLM error: {}", e))?;

        let content = response.message.content.unwrap_or_default();
        self.chat_session.add_assistant_message(&content);

        Ok(content)
    }

    // ── Context Compaction (4-pass: Reclaim → Shrink → Collapse → Evict) ──

    fn compact_if_needed(&mut self) {
        let total_chars: usize = self.chat_session.messages.iter().map(|m| m.content.len()).sum();
        let max_chars = self.chat_session.max_context_chars;

        if total_chars <= max_chars * 80 / 100 {
            return; // Still under 80% threshold, no compaction needed
        }

        let before_count = self.chat_session.messages.len();
        let compactor = soul_compaction::Compactor::new(max_chars);

        // Convert ChatMessage to compaction Messages
        let comp_messages: Vec<soul_compaction::Message> = self
            .chat_session
            .messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    soul_llm::Role::System => soul_compaction::Role::System,
                    soul_llm::Role::User => soul_compaction::Role::User,
                    soul_llm::Role::Assistant => soul_compaction::Role::Assistant,
                    soul_llm::Role::Tool => soul_compaction::Role::Tool,
                };
                soul_compaction::Message::new(role, &m.content)
                    .with_tokens(m.content.len() / 4)
            })
            .collect();

        match compactor.compact(&comp_messages) {
            Ok((compacted, stats)) => {
                // Rebuild chat session from compacted messages
                self.chat_session.messages.clear();
                for msg in &compacted {
                    let role = match msg.role {
                        soul_compaction::Role::System => soul_llm::Role::System,
                        soul_compaction::Role::User => soul_llm::Role::User,
                        soul_compaction::Role::Assistant => soul_llm::Role::Assistant,
                        soul_compaction::Role::Tool => soul_llm::Role::Tool,
                    };
                    self.chat_session.messages.push(soul_llm::ChatMessage {
                        role,
                        content: msg.content.clone(),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }

                tracing::info!(
                    "Compaction: {} → {} messages, {} → {} chars (saved {:.0}%)",
                    before_count,
                    stats.final_count,
                    stats.tokens_before * 4,
                    stats.tokens_after * 4,
                    stats.savings_pct()
                );
            }
            Err(e) => {
                tracing::warn!("Compaction failed, truncating oldest messages: {e}");
                // Fallback: truncate oldest non-system messages
                let sys_count = self.chat_session.messages.iter()
                    .filter(|m| matches!(m.role, soul_llm::Role::System))
                    .count();
                let target = sys_count + (self.chat_session.messages.len() - sys_count) / 2;
                self.chat_session.messages.drain(sys_count..target);
            }
        }
    }

    // ── Memory Distillation ──

    async fn distill_memory(&mut self, task: &str, result: &str) {
        let prompt = format!(
            r#"Distill key learnings from this task execution:

TASK: {task}
RESULT: {result}

Return a JSON object with:
- "facts": array of verified facts learned
- "skills": array of new skills/capabilities discovered
- "key_info": one-line summary for working memory

Only return the JSON, no explanation."#,
            task = task,
            result = truncate_output(result, 500),
        );

        match self.llm.generate(&prompt).await {
            Ok(resp) => {
                match serde_json::from_str::<serde_json::Value>(&resp.response) {
                    Ok(val) => {
                        if let Some(info) = val.get("key_info").and_then(|v| v.as_str()) {
                            self.planner.memory.set_key_info(info);
                        }
                        if let Some(facts) = val.get("facts").and_then(|v| v.as_array()) {
                            for fact in facts {
                                if let Some(f) = fact.as_str() {
                                    self.planner.memory.observe(f.to_string());
                                }
                            }
                        }
                        if let Some(skills) = val.get("skills").and_then(|v| v.as_array()) {
                            tracing::info!("Distilled {} new skills", skills.len());
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Self-distillation JSON parse failed: {e}");
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Self-distillation LLM call failed: {e}");
            }
        }
    }

    // ── Control ──

    pub async fn abort(&self) {
        *self.running.write().await = false;
    }

    pub fn auto_repair(&mut self) -> Vec<String> {
        if !self.config.auto_repair || self.consecutive_failures < self.config.max_consecutive_failures {
            return Vec::new();
        }

        let mut repairs = Vec::new();

        // Reset conversation context (preserve system prompt)
        let system_messages: Vec<soul_llm::ChatMessage> = self.chat_session.messages.iter()
            .filter(|m| matches!(m.role, soul_llm::Role::System))
            .cloned()
            .collect();
        self.chat_session.messages.clear();
        self.chat_session.messages.extend(system_messages);

        // Clear action history
        self.history.clear();

        let msg = format!(
            "SYSTEM: Self-repair triggered after {} consecutive failures (repair #{}) — context reset, continuing fresh.",
            self.consecutive_failures,
            self.repair_count + 1,
        );
        tracing::warn!("{}", msg);
        self.chat_session.add_user_message(&msg);

        self.repair_count += 1;
        self.consecutive_failures = 0;
        repairs.push(msg);

        repairs
    }

    pub fn repair_count(&self) -> usize {
        self.repair_count
    }

    pub fn status(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.config.name,
            "turn": self.turn,
            "max_turns": self.config.max_turns,
            "tools": self.registry.list().len(),
            "success_rate": self.planner.history.success_rate(),
            "observations": self.planner.memory.observations.len(),
            "history": self.history.len(),
            "consecutive_failures": self.consecutive_failures,
            "repair_count": self.repair_count,
            "llm_model": self.llm.config().model,
            "conversation": self.chat_session.history_summary(),
        })
    }
}

// ── System Prompt Builder ──

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

// ── Helpers ──

fn truncate_output(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...[{} more chars]", &s[..max_len], s.len() - max_len)
    }
}

// ── Task Queue ───────────────────────────────────────────────────────

pub struct TaskQueue {
    tx: mpsc::UnboundedSender<TaskRequest>,
    rx: Arc<RwLock<mpsc::UnboundedReceiver<TaskRequest>>>,
}

struct TaskRequest {
    id: String,
    task: String,
    response_tx: oneshot::Sender<Result<String, String>>,
}

impl TaskQueue {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            tx,
            rx: Arc::new(RwLock::new(rx)),
        }
    }

    pub fn submit(&self, task: &str) -> (String, oneshot::Receiver<Result<String, String>>) {
        let id = Uuid::new_v4().to_string();
        let (response_tx, response_rx) = oneshot::channel();
        let _ = self.tx.send(TaskRequest {
            id: id.clone(),
            task: task.to_string(),
            response_tx,
        });
        (id, response_rx)
    }
}

impl Default for TaskQueue {
    fn default() -> Self {
        Self::new()
    }
}

// ── Autonomous Loop (background) ────────────────────────────────────

pub struct AutonomousLoop {
    agent: Arc<RwLock<AutonomousAgent>>,
    goals: Arc<RwLock<Vec<Goal>>>,
    running: Arc<RwLock<bool>>,
}

impl AutonomousLoop {
    pub fn new(agent: AutonomousAgent) -> Self {
        Self {
            agent: Arc::new(RwLock::new(agent)),
            goals: Arc::new(RwLock::new(Vec::new())),
            running: Arc::new(RwLock::new(false)),
        }
    }

    pub async fn add_goal(&self, description: &str, priority: u8) {
        let goal = Goal {
            id: Uuid::new_v4().to_string(),
            description: description.to_string(),
            priority,
            created_at: Utc::now(),
            status: GoalStatus::Active,
        };
        self.goals.write().await.push(goal);
    }

    pub async fn start(&self) {
        *self.running.write().await = true;

        let agent = self.agent.clone();
        let goals = self.goals.clone();
        let running = self.running.clone();

        tokio::spawn(async move {
            while *running.read().await {
                // Check for active goals
                let goal = {
                    let goals = goals.read().await;
                    goals.iter().find(|g| g.status == GoalStatus::Active).cloned()
                };

                if let Some(goal) = goal {
                    tracing::info!("Processing goal: {}", goal.description);

                    let mut agent = agent.write().await;
                    match agent.run_task(&goal.description).await {
                        Ok(result) => {
                            tracing::info!("Goal completed: {}", result);
                            let mut goals = goals.write().await;
                            if let Some(g) = goals.iter_mut().find(|g| g.id == goal.id) {
                                g.status = GoalStatus::Completed;
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Goal failed: {}", e);
                            let mut goals = goals.write().await;
                            if let Some(g) = goals.iter_mut().find(|g| g.id == goal.id) {
                                g.status = GoalStatus::Failed;
                            }
                        }
                    }
                } else {
                    // No active goals, wait
                    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                }
            }
        });
    }

    pub async fn stop(&self) {
        *self.running.write().await = false;
        let agent = self.agent.read().await;
        agent.abort().await;
    }
}
