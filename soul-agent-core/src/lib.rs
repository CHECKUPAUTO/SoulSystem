//! SoulSystem Autonomous Agent Core
//!
//! Implements adaptive reasoning strategies:
//! - **ReAct** (Reason + Act) loop for simple tasks
//! - **PlanThenExecute** for multi-step tasks
//! - **Tree of Thoughts** (ToT) for complex/creative tasks
//!
//! Features:
//! - Conversation context management
//! - Tool dispatch with before/after hooks
//! - Working memory checkpoints
//! - Safety warnings and turn limits
//! - Task queue with abort support
//! - Memory distillation
//! - Adaptive strategy selection with auto-escalation on failures
//!
//! ## Framework API
//!
//! This crate provides a public framework API via traits that can be implemented
//! by external crates:
//!
//! - [`Agent`] — Core agent interface (run_task, ask)
//! - [`Memory`] — Hierarchical memory interface (store, retrieve, search)
//! - [`Tool`] — Tool interface with schema and execution
//! - [`LLMClient`] — Multi-provider LLM interface
//! - [`Planner`] — Goal decomposition and decision making
//!
//! See [`builder`] module for the `AgentBuilder` to compose agents from components.

pub mod builder;
pub mod consolidation;
pub mod finetune;
pub mod parallel;
pub mod prioritization;
pub mod router;
pub mod strategy;
pub mod traits;

use crate::finetune::{DpoPair, FineTuneLoop};
use crate::router::LlmRouter;
use crate::strategy::{StrategyOutcome, StrategySelector, StrategyType};
use ccos::external_memory::{CcosMemory, ExternalMemory, Recall, RecallWindow};
use chrono::Utc;
use soul_error_unifier::ErrorUnifier;
use soul_intrinsic_motivation::IntrinsicMotivation;
use soul_llm::{build_tool_schemas, ChatSession, OllamaClient, ToolSchema};
use soul_memory::{KnowledgeGraph, Node, NodeType};
use soul_planner::{CognitiveLoop, Goal, GoalStatus};
use soul_skills::SkillLoader;
use soul_tools::{async_dispatch_tool, discover_system_tools, AsyncShellExecutor, ToolRegistry};
use soullink_autonomy::{
    error_metrics::{ErrorWeights, GlobalError},
    metacognition::MetaCognition,
    policy_evolution::{ActionSelector, PolicyEvolution, PolicyMetrics, PolicyWeights},
    reward_system::{ActionReward, AgentReward, InformationReward, RewardWeights, SocialReward},
};
use soullink_circuit::{CircuitBreaker, CircuitBreakerConfig};
use soullink_gate::{
    spotlight, ApprovalGate, ApprovalRequirement, ExecutionMode, GateDecision, InjectionScanner,
    RiskLevel, Verdict,
};
use soullink_memory_hierarchy::{
    ConsolidationConfig, EpisodicConfig, HierarchicalMemory, MemoryEntry, SemanticConfig,
};
use soullink_reasoning::{ThoughtTree, TreeConfig};
use soullink_trainer::{Trajectory, TrajectoryRecorder};
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
    pub working_memory_capacity: usize,
    pub enable_sub_agents: bool,
    pub max_sub_agents: usize,
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
            working_memory_capacity: 50,
            enable_sub_agents: true,
            max_sub_agents: 4,
        }
    }
}

// ── Step Outcome ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum StepOutcome {
    Continue {
        next_prompt: Option<String>,
    },
    Done {
        result: String,
    },
    Interrupt {
        question: String,
        candidates: Vec<String>,
    },
    Error {
        message: String,
    },
}

// ── Agent Event (for streaming) ──────────────────────────────────────

#[derive(Debug, Clone)]
pub enum AgentEvent {
    Thinking {
        content: String,
    },
    ToolCall {
        name: String,
        args: serde_json::Value,
    },
    ToolResult {
        name: String,
        output: String,
        success: bool,
    },
    Response {
        content: String,
    },
    SafetyWarning {
        message: String,
    },
    Done {
        summary: String,
    },
    Error {
        message: String,
    },
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
    pub memory: Arc<HierarchicalMemory>,
    pub metacognition: Arc<MetaCognition>,
    pub reasoning: ThoughtTree,
    pub trajectory_recorder: Option<TrajectoryRecorder>,
    pub knowledge_graph: KnowledgeGraph,
    pub skill_loader: Option<Arc<RwLock<SkillLoader>>>,
    pub error_unifier: ErrorUnifier,
    pub motivation: IntrinsicMotivation,
    pub event_tx: Option<mpsc::UnboundedSender<AgentEvent>>,

    // New adaptive components
    pub global_error: GlobalError,
    pub reward_system: AgentReward,
    pub policy_evolution: PolicyEvolution,
    pub action_selector: ActionSelector,
    pub policy_metrics: PolicyMetrics,
    pub strategy_selector: StrategySelector,
    pub last_strategy: StrategyType,
    /// Optional multi-provider router (when absent, uses `self.llm`).
    pub router: Option<Arc<LlmRouter>>,
    /// Circuit breaker to prevent infinite LLM retry loops.
    pub llm_breaker: CircuitBreaker,
    /// Fine-tuning loop for automated model improvement.
    pub ft_loop: Arc<FineTuneLoop>,
    /// Causal context memory (CCOS): files the agent reads are ingested into a
    /// causal graph; failures inject pressure; recall yields a bounded,
    /// causally-coherent working set for long sessions.
    pub ccos: CcosMemory,
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

        let memory = Arc::new(HierarchicalMemory::new(
            config.working_memory_capacity,
            EpisodicConfig::default(),
            SemanticConfig::default(),
            ConsolidationConfig::default(),
        ));

        let metacognition = MetaCognition::new();
        let reasoning = ThoughtTree::new(TreeConfig::default());

        // Initialize adaptive components
        let error_weights = ErrorWeights::default();
        let global_error = GlobalError::new(error_weights);

        let reward_weights = RewardWeights::default();
        let reward_system = AgentReward::new(reward_weights);

        let policy_weights = PolicyWeights::default();
        let policy_evolution = PolicyEvolution::new(0.01, policy_weights);
        let action_selector = ActionSelector::new();
        let policy_metrics = PolicyMetrics::calculate(&policy_evolution);

        let strategy_selector = StrategySelector::default();
        let llm_breaker =
            CircuitBreaker::new("llm", CircuitBreakerConfig::llm_provider(&config.name));

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
            memory,
            metacognition,
            reasoning,
            trajectory_recorder: None,
            knowledge_graph: KnowledgeGraph::new(),
            skill_loader: None,
            error_unifier: ErrorUnifier::new(),
            motivation: IntrinsicMotivation::new(),
            event_tx: None,
            global_error,
            reward_system,
            policy_evolution,
            action_selector,
            policy_metrics,
            strategy_selector,
            last_strategy: StrategyType::ReAct,
            router: None,
            llm_breaker,
            ft_loop: Arc::new(FineTuneLoop::new(Default::default())),
            ccos: CcosMemory::new(),
        }
    }

    /// Construct an agent sharing an external `HierarchicalMemory`.
    pub fn with_memory(
        llm: OllamaClient,
        config: AgentConfig,
        memory: Arc<HierarchicalMemory>,
    ) -> Self {
        let system_prompt = build_system_prompt(&config.name);
        let chat_session = ChatSession::with_max_context(&system_prompt, 40000);

        let tools = discover_system_tools();
        let mut registry = ToolRegistry::new();
        for tool in tools {
            registry.register(tool);
        }

        let tool_schemas = build_tool_schemas();
        let metacognition = MetaCognition::new();
        let reasoning = ThoughtTree::new(TreeConfig::default());

        // Initialize adaptive components
        let error_weights = ErrorWeights::default();
        let global_error = GlobalError::new(error_weights);

        let reward_weights = RewardWeights::default();
        let reward_system = AgentReward::new(reward_weights);

        let policy_weights = PolicyWeights::default();
        let policy_evolution = PolicyEvolution::new(0.01, policy_weights);
        let action_selector = ActionSelector::new();
        let policy_metrics = PolicyMetrics::calculate(&policy_evolution);

        let strategy_selector = StrategySelector::default();
        let llm_breaker =
            CircuitBreaker::new("llm", CircuitBreakerConfig::llm_provider(&config.name));

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
            memory,
            metacognition,
            reasoning,
            trajectory_recorder: None,
            knowledge_graph: KnowledgeGraph::new(),
            skill_loader: None,
            error_unifier: ErrorUnifier::new(),
            motivation: IntrinsicMotivation::new(),
            event_tx: None,
            global_error,
            reward_system,
            policy_evolution,
            action_selector,
            policy_metrics,
            strategy_selector,
            last_strategy: StrategyType::ReAct,
            router: None,
            llm_breaker,
            ft_loop: Arc::new(FineTuneLoop::new(Default::default())),
            ccos: CcosMemory::new(),
        }
    }

    pub fn set_event_sender(&mut self, tx: mpsc::UnboundedSender<AgentEvent>) {
        self.event_tx = Some(tx);
    }

    pub fn set_skill_loader(&mut self, loader: SkillLoader) {
        self.skill_loader = Some(Arc::new(RwLock::new(loader)));
    }

    pub fn set_router(&mut self, router: LlmRouter) {
        self.router = Some(Arc::new(router));
    }

    /// Call the LLM through the circuit breaker. Rejects when breaker is open.
    async fn guarded_llm_chat(
        &self,
        messages: &[soul_llm::ChatMessage],
        tools: Option<&[ToolSchema]>,
    ) -> Result<soul_llm::ChatResponse, String> {
        let llm = self.llm.clone();
        let messages_owned: Vec<soul_llm::ChatMessage> = messages.to_vec();
        let tools_owned: Option<Vec<ToolSchema>> = tools.map(|t| t.to_vec());

        let result = self
            .llm_breaker
            .call(|| {
                let llm = llm.clone();
                let msgs = messages_owned.clone();
                let tools = tools_owned.clone();
                async move {
                    let t = tools.as_ref().map(|v| v.as_slice());
                    match llm.chat(&msgs, t).await {
                        Ok(resp) => Ok(resp),
                        Err(e) => Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))
                            as Box<dyn std::error::Error + Send + Sync>),
                    }
                }
            })
            .await;

        match result {
            Ok(resp) => Ok(resp),
            Err(e) => Err(format!("Circuit breaker: {}", e)),
        }
    }

    /// Route a generate call through the router or fall back.
    async fn routed_generate(&self, prompt: &str) -> Result<soul_llm::ChatResponse, String> {
        match &self.router {
            Some(router) => {
                let complexity = self.strategy_selector.analyze(prompt);
                router
                    .generate(
                        prompt,
                        complexity.domain,
                        complexity.complexity_score,
                        prompt,
                    )
                    .await
            }
            None => self.llm.generate(prompt).await,
        }
    }

    fn emit_event(&self, event: AgentEvent) {
        if let Some(tx) = &self.event_tx {
            let _ = tx.send(event);
        }
    }

    // ── CCOS causal context memory ──

    /// Fold a tool outcome into the CCOS causal graph. File-read tool results
    /// (those carrying a path + source) are ingested so the right code can be
    /// recalled later; failures inject pressure on the implicated file so its
    /// causal neighborhood stays hot.
    fn ccos_observe_tool(&mut self, name: &str, args: &serde_json::Value, output: &str, ok: bool) {
        // Heuristic: a file path the tool acted on (read_file/cat/edit-style tools).
        let path = args
            .get("path")
            .or_else(|| args.get("file"))
            .or_else(|| args.get("filename"))
            .and_then(|v| v.as_str());

        if let Some(p) = path {
            let uri = format!("file:{p}");
            if ok && !output.is_empty() {
                // Successful read/write: ingest the current source.
                let _ = self.ccos.ingest_source(&uri, output);
            } else if !ok {
                // Failure touching this file: signal it (no-op if not yet known).
                let _ = self.ccos.signal_failure(&uri, 3);
            }
        } else if !ok && (output.contains("error[") || output.contains("test failed")) {
            // A build/test failure with no explicit path: bias the hottest node.
            if let Some(node) = self.ccos.hottest_failure_node() {
                let _ = self.ccos.signal_failure(&node, 2);
            }
        }
        let _ = name; // (kept for future per-tool routing)
    }

    /// Recall a bounded, causally-coherent context window for `task` (highest
    /// causal score first). Public so callers/tests can inspect what CCOS would
    /// keep in context.
    pub fn ccos_recall(&self, task: &str, budget_tokens: usize) -> RecallWindow {
        self.ccos.recall(&Recall::task(task), budget_tokens)
    }

    /// Current CCOS graph stats (nodes/edges/events/files/clock).
    pub fn ccos_stats(&self) -> ccos::external_memory::MemoryStats {
        self.ccos.stats()
    }

    // ── Core ReAct Loop ──

    pub async fn run_task(&mut self, task: &str) -> Result<String, String> {
        *self.running.write().await = true;
        self.turn = 0;
        self.chat_session.clear();

        // ── Adaptive Strategy Selection ──
        let strategy = self
            .strategy_selector
            .select_with_failures(task, self.consecutive_failures);
        self.last_strategy = strategy;

        tracing::info!(
            strategy = %strategy,
            failures = self.consecutive_failures,
            task_preview = %truncate_output(task, 80),
            "Strategy selected"
        );

        self.emit_event(AgentEvent::Thinking {
            content: format!(
                "Strategy: {} ({} consecutive failures)",
                strategy, self.consecutive_failures
            ),
        });

        let (result, turns) = match strategy {
            StrategyType::ReAct => {
                let r = self.run_react(task).await;
                (r, self.turn)
            }
            StrategyType::PlanThenExecute => {
                let r = self.run_plan_then_execute(task).await;
                (r, self.turn)
            }
            StrategyType::TreeOfThoughts => {
                let r = self.run_tree_of_thoughts(task).await;
                (r, self.turn)
            }
        };

        *self.running.write().await = false;

        // Post-execution: record outcomes, update learning
        let last_response = match &result {
            Ok(r) => r.clone(),
            Err(_) => String::new(),
        };

        // Record strategy outcome
        self.strategy_selector.record_outcome(StrategyOutcome {
            strategy,
            task_preview: truncate_output(task, 200),
            char_count: task.len(),
            domain: self.strategy_selector.analyze(task).domain,
            estimated_steps: self.strategy_selector.analyze(task).estimated_steps,
            success: result.is_ok(),
            turns_used: turns,
        });

        // Update global error after task completion
        self.update_global_error(task, &last_response).await;

        // Calculate reward for this task execution
        self.calculate_reward(task, &last_response).await;

        // Update policy based on performance
        self.update_policy().await;

        if result.is_err() {
            return result;
        }
        let last_response = result.unwrap();

        // Phase 6: Record trajectory for fine-tuning
        if let Some(ref mut recorder) = self.trajectory_recorder {
            let traj = Trajectory::new(&self.llm.config().model, "q4_k_m", task, &last_response);
            let _ = recorder.record(&traj);
        }

        // Phase 6.5: Submit DPO pair for fine-tuning loop
        if !task.is_empty() && !last_response.is_empty() {
            let critique = soul_critique::quick_critique(task, &last_response);
            let quality = critique.overall_score as f64 / 10.0;
            let dpo = DpoPair {
                prompt: task.to_string(),
                chosen: last_response.clone(),
                rejected: String::new(), // empty = no negative sample yet
                score: quality,
                domain: self.strategy_selector.analyze(task).domain.to_string(),
            };
            self.ft_loop.add_pair(dpo).await;
        }

        // Phase 6: Update metacognition with capability confidence
        self.metacognition
            .register_capability("task_execution", 0.5)
            .await;
        self.metacognition
            .record_outcome("task_execution", !last_response.is_empty())
            .await;

        // Phase 6: Populate knowledge graph
        let task_node =
            Node::new(NodeType::Task, task).with_content(&truncate_output(&last_response, 500));
        self.knowledge_graph.add_node(task_node);

        // Distill learnings
        if self.config.auto_distill && !last_response.is_empty() {
            self.distill_memory(task, &last_response).await;
            self.crystallize_skills(task, &last_response).await;
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

    // ── Strategy: ReAct ───────────────────────────────────────────────

    /// Classic ReAct loop: observe → think → act → evaluate.
    /// Best for simple, single-step tasks.
    async fn run_react(&mut self, task: &str) -> Result<String, String> {
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

            // Inject working memory context + hierarchical memory retrieval
            let memory_context = self.planner.memory.to_prompt_section();
            let mut combined_context = memory_context.clone();

            if !self.planner.memory.key_info.is_empty() {
                let memory_results = self.memory.search(&self.planner.memory.key_info, 3).await;
                if !memory_results.is_empty() {
                    let past: Vec<String> = memory_results
                        .iter()
                        .map(|e| {
                            format!(
                                "[{:?}] {} (importance: {:.2})",
                                e.layer, e.text, e.importance
                            )
                        })
                        .collect();
                    combined_context.push_str("\n\nRelevant past experiences:\n");
                    combined_context.push_str(&past.join("\n"));
                }
            }

            let kg_context = self
                .knowledge_graph
                .context_for_query(&self.planner.memory.key_info, 3);
            if !kg_context.is_empty() {
                combined_context.push_str("\n\nKnowledge graph context:\n");
                combined_context.push_str(&kg_context);
            }

            // Inject metacognition self-model (every 10 turns)
            if self.turn % 10 == 0 {
                let model = self.metacognition.self_model().await;
                combined_context.push_str(&format!(
                    "\n\nSelf-model: health={:.1}%, load={:.1}%, capabilities={}",
                    model.overall_health * 100.0,
                    model.cognitive_load * 100.0,
                    model.capabilities.len(),
                ));
            }

            if !combined_context.is_empty() && self.turn % 5 == 1 {
                self.chat_session.add_user_message(&combined_context);
            }

            self.compact_if_needed();

            let messages = self.chat_session.build_messages();

            self.emit_event(AgentEvent::Thinking {
                content: format!("Turn {}/{}", self.turn, self.config.max_turns),
            });

            let response = match self
                .guarded_llm_chat(&messages, Some(&self.tool_schemas))
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    self.consecutive_failures += 1;
                    // Circuit breaker handles its own state, but we also self-repair
                    let repairs = self.auto_repair();
                    for r in &repairs {
                        self.emit_event(AgentEvent::SafetyWarning { message: r.clone() });
                    }
                    return Err(format!("LLM error: {}", e));
                }
            };

            let msg = &response.message;
            let content = msg.content.clone().unwrap_or_default();

            if let Some(tool_calls) = &msg.tool_calls {
                if !tool_calls.is_empty() {
                    self.chat_session.add_assistant_with_tools(
                        if content.is_empty() {
                            None
                        } else {
                            Some(&content)
                        },
                        tool_calls.clone(),
                    );

                    for tc in tool_calls {
                        let name = tc.function.name.clone();
                        let args: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(serde_json::json!({}));

                        self.emit_event(AgentEvent::ToolCall {
                            name: name.clone(),
                            args: args.clone(),
                        });

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

                        if permission == soul_tools::PermissionLevel::Write {
                            tracing::warn!(
                                "AUDIT: Write-level command: {}({})",
                                name,
                                truncate_output(&args.to_string(), 100)
                            );
                        }

                        let (result, tool_ok) = match async_dispatch_tool(&name, args.clone()).await
                        {
                            Ok(output) => {
                                self.consecutive_failures = 0;
                                self.emit_event(AgentEvent::ToolResult {
                                    name: name.clone(),
                                    output: truncate_output(&output, 2000),
                                    success: true,
                                });
                                (output, true)
                            }
                            Err(e) => {
                                self.consecutive_failures += 1;
                                self.emit_event(AgentEvent::ToolResult {
                                    name: name.clone(),
                                    output: e.clone(),
                                    success: false,
                                });
                                let repairs = self.auto_repair();
                                for r in &repairs {
                                    self.emit_event(AgentEvent::SafetyWarning {
                                        message: r.clone(),
                                    });
                                }
                                (e, false)
                            }
                        };

                        // Feed the causal context memory: ingest any file the tool
                        // surfaced, and inject failure pressure on errors.
                        self.ccos_observe_tool(&name, &args, &result, tool_ok);

                        self.planner.history.record(
                            format!("{}({})", name, truncate_output(&args.to_string(), 100)),
                            truncate_output(&result, 200),
                            true,
                        );

                        self.chat_session
                            .add_tool_result(&tc.id, &truncate_output(&result, 3000));
                    }

                    continue;
                }
            }

            if !content.is_empty() {
                last_response = content.clone();
                self.chat_session.add_assistant_message(&content);
                self.emit_event(AgentEvent::Response {
                    content: content.clone(),
                });
            }

            let lower = last_response.to_lowercase();
            if lower.contains("task completed")
                || lower.contains("done")
                || lower.contains("finished")
                || lower.contains("completed successfully")
            {
                break;
            }

            if content.is_empty() && msg.tool_calls.is_none() {
                break;
            }
        }

        Ok(last_response)
    }

    // ── Strategy: Plan Then Execute ────────────────────────────────────

    /// Plan-first strategy: decompose the task into steps, then execute
    /// each step sequentially with ReAct. Best for multi-step tasks.
    async fn run_plan_then_execute(&mut self, task: &str) -> Result<String, String> {
        // Phase 1: Create a plan via LLM
        let plan_prompt = format!(
            r#"You are a task planner. Break down the following task into clear, numbered steps.
Each step should be a single, actionable instruction.

TASK: {task}

Return ONLY a numbered list of steps, one per line:
1. First step
2. Second step
...
N. Final step"#,
            task = task
        );

        let plan_response = self
            .llm
            .generate(&plan_prompt)
            .await
            .map_err(|e| format!("Plan generation failed: {}", e))?;

        let plan_text = plan_response.message.content.unwrap_or_default();
        let steps: Vec<String> = plan_text
            .lines()
            .filter(|l| {
                let trimmed = l.trim();
                !trimmed.is_empty()
                    && trimmed
                        .chars()
                        .next()
                        .map(|c| c.is_ascii_digit())
                        .unwrap_or(false)
            })
            .map(|l| {
                // Strip the leading number and dot/paren
                let trimmed = l.trim();
                let after_num = trimmed
                    .find(|c: char| c == '.' || c == ')' || c == ':')
                    .map(|i| &trimmed[i + 1..])
                    .unwrap_or(trimmed);
                after_num.trim().to_string()
            })
            .collect();

        let step_count = steps.len();
        self.emit_event(AgentEvent::Thinking {
            content: format!("Plan created: {} steps", step_count),
        });
        tracing::info!(steps = step_count, "PlanThenExecute plan created");

        // Phase 2: Execute each step as a sub-task with ReAct
        let mut all_results = Vec::new();

        for (i, step) in steps.iter().enumerate() {
            if !*self.running.read().await {
                return Err("Task aborted".to_string());
            }

            let step_task = format!(
                "Context: You are executing a plan for the task: {task}\n\nCurrent step ({current}/{total}): {step}\n\nPrevious results: {prev}\n\nExecute this step now.",
                task = task,
                current = i + 1,
                total = step_count,
                step = step,
                prev = all_results
                    .last()
                    .map(|r: &String| truncate_output(r, 200))
                    .unwrap_or_else(|| "none".to_string()),
            );

            self.emit_event(AgentEvent::Thinking {
                content: format!(
                    "Step {}/{}: {}",
                    i + 1,
                    step_count,
                    truncate_output(step, 60)
                ),
            });

            self.chat_session.clear();
            self.turn = 0; // reset turns for each sub-task

            match self.run_react(&step_task).await {
                Ok(result) => {
                    all_results.push(format!("Step {} result: {}", i + 1, result));
                    self.planner.memory.set_key_info(&result);
                }
                Err(e) => {
                    all_results.push(format!("Step {} FAILED: {}", i + 1, e));
                    // Continue with remaining steps
                }
            }
        }

        // Phase 3: Synthesize final result
        let synthesis_prompt = format!(
            "Task: {task}\n\nStep results:\n{all}\n\nSummarize the overall outcome in 2-3 sentences.",
            task = task,
            all = all_results.join("\n")
        );

        let final_response = self
            .llm
            .generate(&synthesis_prompt)
            .await
            .map_err(|e| format!("Synthesis failed: {}", e))?;

        let summary = final_response.message.content.unwrap_or_default();
        Ok(summary)
    }

    // ── Strategy: Tree of Thoughts ────────────────────────────────────

    /// Tree of Thoughts: explore multiple reasoning paths, evaluate
    /// each with semantic similarity, and prune low-scoring branches.
    /// Best for creative, design, debug, and complex research tasks.
    async fn run_tree_of_thoughts(&mut self, task: &str) -> Result<String, String> {
        // Build the thought tree
        let config = TreeConfig {
            max_depth: self.strategy_selector.config.tot_max_depth,
            max_branches: self.strategy_selector.config.tot_max_branches,
            accept_threshold: 0.4,
            learning_rate: 0.05,
            top_k: self.strategy_selector.config.tot_top_k,
        };
        self.reasoning = ThoughtTree::new(config);

        // Phase 1: Generate initial thought branches via LLM
        let root_id = self.reasoning.add_root(task);

        let branch_prompt = format!(
            "For the task below, generate {n} different possible approaches or reasoning paths. Each approach should be a distinct angle.\n\nTASK: {task}\n\nReturn exactly {n} approaches, each on a new line starting with '- '",
            n = self.strategy_selector.config.tot_max_branches,
            task = task
        );

        match self.llm.generate(&branch_prompt).await {
            Ok(resp) => {
                let text = resp.message.content.unwrap_or_default();
                for line in text.lines() {
                    let trimmed = line.trim().strip_prefix("- ").unwrap_or(line.trim());
                    if !trimmed.is_empty() {
                        self.reasoning.add_child(root_id, trimmed);
                    }
                }
            }
            Err(e) => {
                // Fallback: add generic branches
                self.reasoning
                    .add_child(root_id, format!("Analyze: {}", task));
                self.reasoning
                    .add_child(root_id, format!("Decompose: {}", task));
                self.reasoning
                    .add_child(root_id, format!("Research: {}", task));
                tracing::warn!("ToT branch generation failed, using fallback: {}", e);
            }
        }

        // Phase 2: Explore and evaluate each branch
        let mut depth = 0;
        while depth < self.strategy_selector.config.tot_max_depth {
            if !*self.running.read().await {
                return Err("Task aborted".to_string());
            }

            // For each accepted leaf node, generate child thoughts
            let leaf_ids: Vec<usize> = {
                let mut ids = Vec::new();
                // We need to iterate all nodes to find leaves
                // Use best_path as proxy for iteration
                for id in 0..self.reasoning.len() + 10 {
                    if let Some(node) = self.reasoning.get(id) {
                        if node.is_leaf()
                            && node.status != soullink_reasoning::node::NodeStatus::Pruned
                        {
                            ids.push(id);
                        }
                    }
                }
                ids
            };

            if leaf_ids.is_empty() {
                break;
            }

            // Evaluate and expand each leaf
            for leaf_id in leaf_ids {
                if !*self.running.read().await {
                    return Err("Task aborted".to_string());
                }

                let leaf_content = self
                    .reasoning
                    .get(leaf_id)
                    .map(|n| n.content.clone())
                    .unwrap_or_default();

                let expand_prompt = format!(
                    "Given the approach: \"{leaf}\"\n\nGenerate {n} more specific subtasks or deeper reasoning paths.\nReturn each on a new line starting with '- '.",
                    leaf = leaf_content,
                    n = self.strategy_selector.config.tot_max_branches
                );

                match self.llm.generate(&expand_prompt).await {
                    Ok(resp) => {
                        let text = resp.message.content.unwrap_or_default();
                        let mut added = 0;
                        for line in text.lines() {
                            let trimmed = line.trim().strip_prefix("- ").unwrap_or(line.trim());
                            if !trimmed.is_empty()
                                && added < self.strategy_selector.config.tot_max_branches
                            {
                                self.reasoning.add_child(leaf_id, trimmed);
                                added += 1;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("ToT expansion failed for leaf {}: {}", leaf_id, e);
                    }
                }

                // Evaluate using semantic loss (using embeddings if available, else fake)
                // In production, use soullink-eval with real embeddings
                let query_emb = vec![1.0_f32; 64]; // placeholder
                let thought_emb = vec![1.0_f32; 64]; // placeholder
                let _ = self
                    .reasoning
                    .evaluate_node(leaf_id, &query_emb, &thought_emb);

                self.turn += 1;
                self.emit_event(AgentEvent::Thinking {
                    content: format!(
                        "ToT depth {}/{} : {} nodes explored",
                        depth + 1,
                        self.strategy_selector.config.tot_max_depth,
                        self.reasoning.len()
                    ),
                });
            }

            // Prune low-scoring nodes
            let pruned = self.reasoning.prune();
            if pruned > 0 {
                tracing::info!("ToT pruned {} nodes", pruned);
            }

            depth += 1;
        }

        // Phase 3: Extract best path as the result
        let best_path = self.reasoning.best_path();
        let reasoning_chain: Vec<String> = best_path.iter().map(|n| n.content.clone()).collect();
        let pruned_count = self.reasoning.pruned_count();
        let accepted_count = self.reasoning.accepted_count();

        if reasoning_chain.is_empty() {
            return Err("TreeOfThoughts: no valid reasoning path found".to_string());
        }

        // Synthesize final answer from best reasoning chain
        let synthesis_prompt = format!(
            "Task: {task}\n\nReasoning chain:\n{chain}\n\nBased on this analysis, provide a clear final answer in 3-5 sentences.",
            task = task,
            chain = reasoning_chain.join("\n → ")
        );

        match self.llm.generate(&synthesis_prompt).await {
            Ok(resp) => {
                let result = resp.message.content.unwrap_or_default();
                tracing::info!(
                    accepted = accepted_count,
                    pruned = pruned_count,
                    total_nodes = self.reasoning.len(),
                    "TreeOfThoughts completed"
                );
                Ok(result)
            }
            Err(e) => {
                // Fallback: return the best path's content
                Ok(format!(
                    "ToT analysis ({} accepted, {} pruned):\n{}",
                    accepted_count,
                    pruned_count,
                    reasoning_chain.join("\n")
                ))
            }
        }
    }

    // ── Interactive Ask ──

    pub async fn ask(&mut self, question: &str) -> Result<String, String> {
        self.chat_session.add_user_message(question);

        // Auto-compact before building messages if context is large
        self.compact_if_needed();

        let messages = self.chat_session.build_messages();

        let response = self
            .guarded_llm_chat(&messages, Some(&self.tool_schemas))
            .await
            .map_err(|e| format!("LLM error: {}", e))?;

        let content = response.message.content.unwrap_or_default();
        self.chat_session.add_assistant_message(&content);

        Ok(content)
    }

    // ── Context Compaction (4-pass: Reclaim → Shrink → Collapse → Evict) ──

    fn compact_if_needed(&mut self) {
        let total_chars: usize = self
            .chat_session
            .messages
            .iter()
            .map(|m| m.content.len())
            .sum();
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
                soul_compaction::Message::new(role, &m.content).with_tokens(m.content.len() / 4)
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
                let sys_count = self
                    .chat_session
                    .messages
                    .iter()
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
            Ok(resp) => match serde_json::from_str::<serde_json::Value>(
                resp.message.content.as_deref().unwrap_or(""),
            ) {
                Ok(val) => {
                    if let Some(info) = val.get("key_info").and_then(|v| v.as_str()) {
                        self.planner.memory.set_key_info(info);
                    }
                    if let Some(facts) = val.get("facts").and_then(|v| v.as_array()) {
                        for fact in facts {
                            if let Some(f) = fact.as_str() {
                                self.planner.memory.observe(f.to_string());
                                let entry = MemoryEntry {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    text: f.to_string(),
                                    created_at: Utc::now().to_rfc3339(),
                                    last_accessed: Utc::now().to_rfc3339(),
                                    access_count: 1,
                                    importance: 0.5,
                                    layer: soullink_memory_hierarchy::MemoryLayer::Episodic,
                                    tags: vec!["distilled".to_string()],
                                    embedding: None,
                                    associations: vec![],
                                    metadata: HashMap::new(),
                                };
                                self.memory
                                    .store(entry, soullink_memory_hierarchy::MemoryLayer::Episodic)
                                    .await;
                            }
                        }
                    }
                    if let Some(skills) = val.get("skills").and_then(|v| v.as_array()) {
                        tracing::info!("Distilled {} new skills", skills.len());
                        for skill in skills {
                            if let Some(s) = skill.as_str() {
                                let entry = MemoryEntry {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    text: format!("[SKILL] {}", s),
                                    created_at: Utc::now().to_rfc3339(),
                                    last_accessed: Utc::now().to_rfc3339(),
                                    access_count: 1,
                                    importance: 0.7,
                                    layer: soullink_memory_hierarchy::MemoryLayer::Semantic,
                                    tags: vec!["skill".to_string()],
                                    embedding: None,
                                    associations: vec![],
                                    metadata: HashMap::new(),
                                };
                                self.memory
                                    .store(entry, soullink_memory_hierarchy::MemoryLayer::Semantic)
                                    .await;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Self-distillation JSON parse failed: {e}");
                }
            },
            Err(e) => {
                tracing::warn!("Self-distillation LLM call failed: {e}");
            }
        }
    }

    async fn crystallize_skills(&self, task: &str, result: &str) {
        let loader = match &self.skill_loader {
            Some(l) => l,
            None => return,
        };

        let prompt = format!(
            r#"Crystallize reusable skills from this task execution:

TASK: {task}
RESULT: {result}

Return a JSON array of skill objects. Each skill has:
- "name": short skill name
- "description": one-line description
- "triggers": array of trigger phrases that would invoke this skill
- "steps": array of step-by-step instructions
- "tags": array of category tags

Only return the JSON array, no explanation."#,
            task = task,
            result = truncate_output(result, 500),
        );

        match self.llm.generate(&prompt).await {
            Ok(resp) => match serde_json::from_str::<Vec<serde_json::Value>>(
                resp.message.content.as_deref().unwrap_or(""),
            ) {
                Ok(skills) => {
                    let mut crystallized = 0;
                    for skill_val in &skills {
                        let name = skill_val
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unnamed");
                        let description = skill_val
                            .get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let triggers: Vec<String> = skill_val
                            .get("triggers")
                            .and_then(|v| v.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();
                        let steps: Vec<String> = skill_val
                            .get("steps")
                            .and_then(|v| v.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();

                        if triggers.is_empty() || steps.is_empty() {
                            continue;
                        }

                        let skill = soul_skills::Skill::new(name, description);
                        let skill = soul_skills::Skill {
                            triggers,
                            steps,
                            tags: skill_val
                                .get("tags")
                                .and_then(|v| v.as_array())
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|v| v.as_str().map(String::from))
                                        .collect()
                                })
                                .unwrap_or_default(),
                            ..skill
                        };

                        let loader_lock = loader.read().await;
                        if let Err(e) = loader_lock.save_skill(&skill).await {
                            tracing::warn!("Skill save failed for '{}': {:?}", name, e);
                        } else {
                            crystallized += 1;
                        }
                    }
                    if crystallized > 0 {
                        tracing::info!("Crystallized {} new skills from task", crystallized);
                    }
                }
                Err(e) => {
                    tracing::warn!("Skill crystallization JSON parse failed: {e}");
                }
            },
            Err(e) => {
                tracing::warn!("Skill crystallization LLM call failed: {e}");
            }
        }
    }

    // ── Control ──

    pub async fn abort(&self) {
        *self.running.write().await = false;
    }

    pub fn auto_repair(&mut self) -> Vec<String> {
        if !self.config.auto_repair
            || self.consecutive_failures < self.config.max_consecutive_failures
        {
            return Vec::new();
        }

        let mut repairs = Vec::new();

        // Reset conversation context (preserve system prompt)
        let system_messages: Vec<soul_llm::ChatMessage> = self
            .chat_session
            .messages
            .iter()
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

    /// Update global error metrics after task completion.
    async fn update_global_error(&mut self, task: &str, result: &str) {
        // Calculate prediction error (simplified for this example)
        let prediction_error = 0.1; // In a real implementation, this would be calculated from predictions vs actual
        self.global_error.update_prediction_error(prediction_error);

        // Calculate action error (reinforcement learning style)
        let action_error = 0.05; // Simplified - in practice this would be calculated using Q-learning
        self.global_error.update_action_error(action_error);

        // Calculate goal error
        let goal_error = 0.2; // Simplified - actual implementation would compare with target goals
        self.global_error.update_goal_error(goal_error);

        // Calculate social error (if applicable)
        let social_error = 0.15; // Simplified
        self.global_error.update_social_error(social_error);

        // Calculate uncertainty
        let uncertainty = 0.3; // Simplified - actual implementation would use entropy or confidence metrics
        self.global_error.update_uncertainty(uncertainty);

        // Calculate initiative error (if applicable)
        let initiative_error = 0.1; // Simplified
        self.global_error.update_initiative_error(initiative_error);

        // Recalculate global error
        self.global_error.calculate();
    }

    /// Calculate reward for task execution.
    async fn calculate_reward(&mut self, task: &str, result: &str) {
        // Action reward calculation
        let action_reward = ActionReward::calculate(
            !result.is_empty(),           // completed successfully
            0.8,                          // quality score (simplified)
            self.turn as f64,             // execution time (simplified)
            self.config.max_turns as f64, // max allowed time (simplified)
        );
        self.reward_system
            .update_action_reward(action_reward.total());

        // Social reward calculation
        let social_reward = SocialReward::calculate(
            0.7, // performance improvement (simplified)
            0.8, // risk avoidance (simplified)
            0.9, // knowledge gain (simplified)
            0.8, // collaboration score (simplified)
        );
        self.reward_system
            .update_social_reward(social_reward.total());

        // Information reward calculation
        let information_reward = InformationReward::calculate(
            0.8, // uncertainty before (simplified)
            0.3, // uncertainty after (simplified)
            0.7, // performance improvement (simplified)
            0.9, // novelty score (simplified)
        );
        self.reward_system
            .update_information_reward(information_reward.total());

        // Recalculate total reward
        self.reward_system.calculate();
    }

    /// Update policy based on recent performance.
    async fn update_policy(&mut self) {
        // Get current policy parameters
        let exploration_rate = self
            .policy_evolution
            .get_parameter("exploration_rate")
            .unwrap_or(0.1);
        let exploitation_rate = self
            .policy_evolution
            .get_parameter("exploitation_rate")
            .unwrap_or(0.9);

        // Use global error and reward to update policy
        let error = self.global_error.global_error;
        let reward = self.reward_system.total_reward;

        // Simplified policy update - in practice this would be more complex
        self.policy_evolution.update_policy(
            reward,      // Q-value (simplified)
            1.0 - error, // Goal alignment (inverted error)
            error,       // Global error
            0.5,         // Uncertainty (simplified)
            0.7,         // Social factor (simplified)
        );

        // Update policy metrics
        self.policy_metrics = PolicyMetrics::calculate(&self.policy_evolution);
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
            "global_error": self.global_error.global_error,
            "total_reward": self.reward_system.total_reward,
            "policy_stability": self.policy_metrics.policy_stability,
            "last_strategy": self.last_strategy.to_string(),
            "strategy_performance": self.strategy_selector.performance_summary(),
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
    #[allow(dead_code)]
    rx: Arc<RwLock<mpsc::UnboundedReceiver<TaskRequest>>>,
}

struct TaskRequest {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    task: String,
    #[allow(dead_code)]
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
                    goals
                        .iter()
                        .find(|g| g.status == GoalStatus::Active)
                        .cloned()
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

    /// Get current policy metrics.
    pub async fn get_policy_metrics(&self) -> PolicyMetrics {
        let agent = self.agent.read().await;
        agent.policy_metrics.clone()
    }

    /// Get current global error.
    pub async fn get_global_error(&self) -> f64 {
        let agent = self.agent.read().await;
        agent.global_error.global_error
    }

    /// Get current reward.
    pub async fn get_total_reward(&self) -> f64 {
        let agent = self.agent.read().await;
        agent.reward_system.total_reward
    }

    pub async fn stop(&self) {
        *self.running.write().await = false;
        let agent = self.agent.read().await;
        agent.abort().await;
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── CCOS causal context memory ──────────────────────────────────────

    #[test]
    fn ccos_ingests_file_reads_and_recalls_causal_window() {
        let mut agent = make_test_agent();
        // Simulate the agent reading a 3-file dependency chain via a read tool.
        let read = |p: &str, src: &str| (serde_json::json!({ "path": p }), src.to_string());
        let chain = [
            (
                "src/main.rs",
                "use crate::handler;\nfn main() { handler::handle(); }\n",
            ),
            (
                "src/handler.rs",
                "use crate::db;\npub fn handle() { db::query(); }\n",
            ),
            ("src/db.rs", "pub fn query() -> i64 { 0 }\n"),
        ];
        for (p, src) in chain {
            let (args, out) = read(p, src);
            agent.ccos_observe_tool("read_file", &args, &out, true);
        }

        // The causal graph now has the files.
        assert!(agent.ccos_stats().files >= 3, "files should be ingested");

        // Recall for a task about db.rs brings back the causal neighborhood.
        let window = agent.ccos_recall("fix the query() bug in db.rs", 4096);
        let uris: Vec<&str> = window.items.iter().map(|i| i.uri.as_str()).collect();
        assert!(
            uris.iter().any(|u| u.contains("db.rs")),
            "recall window must contain db.rs, got {uris:?}"
        );
    }

    #[test]
    fn ccos_failure_signal_is_safe_when_unknown() {
        let mut agent = make_test_agent();
        // Failure on an unknown file must not panic / must be a no-op signal.
        let args = serde_json::json!({ "path": "src/missing.rs" });
        agent.ccos_observe_tool("edit_file", &args, "error[E0001]: boom", false);
        // Graph is still consistent.
        assert!(agent.ccos.verify().valid);
    }

    // ── Mock helpers ────────────────────────────────────────────────────

    /// Creates a minimal AutonomousAgent for testing logic that doesn't need real LLM calls.
    fn make_test_agent() -> AutonomousAgent {
        use std::time::Duration;
        let llm_config = soul_llm::LlmConfig {
            provider: soul_llm::ProviderKind::Ollama,
            base_url: "http://localhost:11888".to_string(), // unreachable, but we never call it
            model: "test-model".to_string(),
            temperature: 0.5,
            max_tokens: 1024,
            http_timeout: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(1),
            auth_token: None,
            goal_token_budget: 1000,
            tokens_per_minute_budget: 10000,
            pool_max_idle: 1,
            pool_idle_timeout: Duration::from_secs(30),
        };
        let llm = OllamaClient::new(llm_config);
        let config = AgentConfig {
            name: "TestAgent".to_string(),
            max_turns: 10,
            max_tool_retries: 1,
            shell_timeout_secs: 5,
            safety_warning_turns: vec![3, 7],
            auto_distill: false,
            auto_repair: true,
            max_consecutive_failures: 2,
            working_memory_capacity: 10,
            enable_sub_agents: false,
            max_sub_agents: 0,
        };
        AutonomousAgent::new(llm, config)
    }

    // ── AgentConfig ─────────────────────────────────────────────────────

    #[test]
    fn test_agent_config_default() {
        let cfg = AgentConfig::default();
        assert_eq!(cfg.name, "SoulSystem");
        assert_eq!(cfg.max_turns, 50);
        assert_eq!(cfg.max_tool_retries, 3);
        assert_eq!(cfg.shell_timeout_secs, 60);
        assert_eq!(cfg.safety_warning_turns, vec![7, 10, 15, 25, 35, 50]);
        assert!(cfg.auto_distill);
        assert!(cfg.auto_repair);
        assert_eq!(cfg.max_consecutive_failures, 3);
        assert_eq!(cfg.working_memory_capacity, 50);
        assert!(cfg.enable_sub_agents);
        assert_eq!(cfg.max_sub_agents, 4);
    }

    #[test]
    fn test_agent_config_custom() {
        let cfg = AgentConfig {
            name: "MyBot".to_string(),
            max_turns: 100,
            max_tool_retries: 5,
            shell_timeout_secs: 30,
            safety_warning_turns: vec![],
            auto_distill: false,
            auto_repair: false,
            max_consecutive_failures: 1,
            working_memory_capacity: 200,
            enable_sub_agents: false,
            max_sub_agents: 10,
        };
        assert_eq!(cfg.name, "MyBot");
        assert_eq!(cfg.max_turns, 100);
        assert_eq!(cfg.auto_distill, false);
        assert_eq!(cfg.auto_repair, false);
    }

    // ── truncate_output ─────────────────────────────────────────────────

    #[test]
    fn test_truncate_output_short_string() {
        let result = truncate_output("hello world", 100);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_truncate_output_exact_fit() {
        let result = truncate_output("exact", 5);
        assert_eq!(result, "exact");
    }

    #[test]
    fn test_truncate_output_long_string() {
        let input = "a".repeat(200);
        let result = truncate_output(&input, 50);
        assert!(result.starts_with("a".repeat(50).as_str()));
        assert!(result.contains("..."));
        assert!(result.contains("150 more chars"));
        assert_eq!(result.len(), 50 + 3 + 16); // 50 chars + ...[ + number + " more chars"]
    }

    #[test]
    fn test_truncate_output_empty_string() {
        let result = truncate_output("", 100);
        assert_eq!(result, "");
    }

    #[test]
    fn test_truncate_output_zero_max() {
        let result = truncate_output("hello", 0);
        assert_eq!(result, "...[5 more chars]");
    }

    // ── build_system_prompt ─────────────────────────────────────────────

    #[test]
    fn test_build_system_prompt_includes_name() {
        let prompt = build_system_prompt("TestBot");
        assert!(prompt.contains("You are TestBot"));
        assert!(prompt.contains("execute_shell"));
        assert!(prompt.contains("file"));
        assert!(prompt.contains("tool calling"));
        assert!(prompt.contains("Task completed"));
    }

    #[test]
    fn test_build_system_prompt_empty_name() {
        let prompt = build_system_prompt("");
        assert!(prompt.contains("You are "));
    }

    // ── auto_repair ─────────────────────────────────────────────────────

    #[test]
    fn test_auto_repair_not_triggered_below_threshold() {
        let mut agent = make_test_agent();
        agent.consecutive_failures = 1; // below max_consecutive_failures (2)
        agent.config.auto_repair = true;
        let repairs = agent.auto_repair();
        assert!(
            repairs.is_empty(),
            "repair should not trigger below threshold"
        );
        assert_eq!(agent.repair_count, 0);
    }

    #[test]
    fn test_auto_repair_triggered_at_threshold() {
        let mut agent = make_test_agent();
        agent.consecutive_failures = 2; // == max_consecutive_failures
        agent.config.auto_repair = true;
        let repairs = agent.auto_repair();
        assert!(!repairs.is_empty(), "repair should trigger at threshold");
        assert_eq!(agent.repair_count, 1);
        assert_eq!(agent.consecutive_failures, 0);
        // Should have added a repair message
        assert!(!agent.chat_session.messages.is_empty());
        assert_eq!(
            agent.chat_session.messages.last().unwrap().role,
            soul_llm::Role::User,
            "repair should add a user message explaining the reset"
        );
        assert!(
            agent
                .chat_session
                .messages
                .last()
                .unwrap()
                .content
                .contains("Self-repair triggered"),
            "repair message should explain the reset"
        );
    }

    #[test]
    fn test_auto_repair_disabled() {
        let mut agent = make_test_agent();
        agent.consecutive_failures = 5;
        agent.config.auto_repair = false;
        let repairs = agent.auto_repair();
        assert!(
            repairs.is_empty(),
            "repair should not trigger when disabled"
        );
        assert_eq!(agent.repair_count, 0);
    }

    #[test]
    fn test_auto_repair_multiple_calls_increment_count() {
        let mut agent = make_test_agent();
        agent.consecutive_failures = 2;
        agent.config.auto_repair = true;

        let r1 = agent.auto_repair();
        assert_eq!(agent.repair_count, 1);
        assert!(!r1.is_empty());
        assert!(r1[0].contains("repair #1"));

        // Build up failures again
        agent.consecutive_failures = 2;
        let r2 = agent.auto_repair();
        assert_eq!(agent.repair_count, 2);
        assert!(r2[0].contains("repair #2"));
    }

    #[test]
    fn test_auto_repair_clears_history() {
        let mut agent = make_test_agent();
        agent.history.push("some history".to_string());
        agent.consecutive_failures = 2;
        agent.config.auto_repair = true;

        agent.auto_repair();
        assert!(
            agent.history.is_empty(),
            "history should be cleared after repair"
        );
    }

    // ── TaskQueue ───────────────────────────────────────────────────────

    #[test]
    fn test_task_queue_default() {
        let queue = TaskQueue::default();
        // Default creates a new channel; submit should work
        let (id, _rx) = queue.submit("test task");
        assert!(!id.is_empty(), "task id should not be empty");
    }

    #[test]
    fn test_task_queue_submit_returns_valid_uuid() {
        let queue = TaskQueue::new();
        let (id, _rx) = queue.submit("hello");
        // Should be parseable as UUID
        let parsed = uuid::Uuid::parse_str(&id);
        assert!(parsed.is_ok(), "task id should be a valid UUID");
    }

    #[test]
    fn test_task_queue_submit_multiple_tasks() {
        let queue = TaskQueue::new();
        let (id1, _rx1) = queue.submit("task one");
        let (id2, _rx2) = queue.submit("task two");
        assert_ne!(id1, id2, "each task should have a unique id");
    }

    #[tokio::test]
    async fn test_task_queue_submit_can_receive_id() {
        let queue = TaskQueue::new();
        let (id, _rx) = queue.submit("test");
        // Just verify we get a valid UUID back
        assert!(!id.is_empty());
        let parsed = uuid::Uuid::parse_str(&id);
        assert!(parsed.is_ok());
    }

    // ── AutonomousLoop add_goal ─────────────────────────────────────────

    #[tokio::test]
    async fn test_autonomous_loop_add_goal() {
        let agent = make_test_agent();
        let loop_ = AutonomousLoop::new(agent);

        loop_.add_goal("Test goal 1", 5).await;
        let goals = loop_.goals.read().await;
        assert_eq!(goals.len(), 1);
        assert_eq!(goals[0].description, "Test goal 1");
        assert_eq!(goals[0].priority, 5);
        assert_eq!(goals[0].status, GoalStatus::Active);
    }

    #[tokio::test]
    async fn test_autonomous_loop_add_multiple_goals() {
        let agent = make_test_agent();
        let loop_ = AutonomousLoop::new(agent);

        loop_.add_goal("Goal A", 1).await;
        loop_.add_goal("Goal B", 2).await;
        loop_.add_goal("Goal C", 3).await;

        let goals = loop_.goals.read().await;
        assert_eq!(goals.len(), 3);
        assert!(goals.iter().all(|g| g.status == GoalStatus::Active));
    }

    #[tokio::test]
    async fn test_autonomous_loop_goal_ids_are_unique() {
        let agent = make_test_agent();
        let loop_ = AutonomousLoop::new(agent);

        loop_.add_goal("Alpha", 1).await;
        loop_.add_goal("Beta", 2).await;

        let goals = loop_.goals.read().await;
        assert_ne!(goals[0].id, goals[1].id);
    }

    #[tokio::test]
    async fn test_autonomous_loop_default_starts_not_running() {
        let agent = make_test_agent();
        let loop_ = AutonomousLoop::new(agent);
        assert!(!*loop_.running.read().await);
    }

    // ── status() ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_status_returns_json_with_all_fields() {
        let agent = make_test_agent();
        let status = agent.status();

        assert_eq!(status["name"], "TestAgent");
        assert_eq!(status["turn"], 0);
        assert_eq!(status["max_turns"], 10);
        assert!(status["tools"].is_number());
        assert!(status["success_rate"].is_number());
        assert!(status["observations"].is_number());
        assert!(status["history"].is_number());
        assert!(status["consecutive_failures"].is_number());
        assert!(status["repair_count"].is_number());
        assert_eq!(status["llm_model"], "test-model");
        assert!(status["conversation"].is_string());
    }

    #[tokio::test]
    async fn test_status_reflects_turn_state() {
        let mut agent = make_test_agent();
        agent.turn = 7;
        agent.consecutive_failures = 2;
        agent.repair_count = 3;

        let status = agent.status();
        assert_eq!(status["turn"], 7);
        assert_eq!(status["consecutive_failures"], 2);
        assert_eq!(status["repair_count"], 3);
    }

    #[tokio::test]
    async fn test_status_serializes_to_valid_json() {
        let agent = make_test_agent();
        let status = agent.status();
        let json_str = serde_json::to_string(&status).unwrap();
        // Verify it round-trips
        let deserialized: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized["name"], "TestAgent");
    }

    // ── compact_if_needed (under threshold) ─────────────────────────────

    #[tokio::test]
    async fn test_compact_if_needed_under_threshold_does_nothing() {
        let mut agent = make_test_agent();
        // session starts empty, total_chars = 0 < 80% of max_context_chars => no-op
        let before_count = agent.chat_session.messages.len();
        agent.compact_if_needed();
        assert_eq!(
            agent.chat_session.messages.len(),
            before_count,
            "compact should not run when context is under 80% threshold"
        );
    }

    #[tokio::test]
    async fn test_compact_if_needed_small_context_unchanged() {
        let mut agent = make_test_agent();
        agent.chat_session.add_user_message("short message");
        agent.chat_session.add_assistant_message("short reply");

        let before = agent.chat_session.messages.len();
        agent.compact_if_needed();
        // Still well under 80% of 40000
        assert_eq!(agent.chat_session.messages.len(), before);
    }

    #[tokio::test]
    async fn test_compact_if_needed_preserves_content_under_threshold() {
        let mut agent = make_test_agent();
        agent.chat_session.add_user_message("Hello, how are you?");
        agent
            .chat_session
            .add_assistant_message("I am doing great, thank you!");

        let content_before: Vec<String> = agent
            .chat_session
            .messages
            .iter()
            .map(|m| m.content.clone())
            .collect();

        agent.compact_if_needed();

        let content_after: Vec<String> = agent
            .chat_session
            .messages
            .iter()
            .map(|m| m.content.clone())
            .collect();

        assert_eq!(
            content_before, content_after,
            "content should be unchanged when under threshold"
        );
    }

    // ── AutonomousAgent constructor ─────────────────────────────────────

    #[test]
    fn test_agent_constructor_sets_defaults() {
        let agent = make_test_agent();
        assert_eq!(agent.turn, 0);
        assert_eq!(agent.consecutive_failures, 0);
        assert_eq!(agent.repair_count, 0);
        assert!(agent.tool_schemas.len() > 0, "should have tool schemas");
    }

    #[tokio::test]
    async fn test_agent_constructor_running_flag() {
        let agent = make_test_agent();
        // After construction, running should be false
        assert!(!*agent.running.read().await);
        // Confirm config is set correctly
        assert_eq!(agent.config.name, "TestAgent");
    }

    // ── StepOutcome ─────────────────────────────────────────────────────

    #[test]
    fn test_step_outcome_variants() {
        let cont = StepOutcome::Continue { next_prompt: None };
        let done = StepOutcome::Done {
            result: "finished".into(),
        };
        let interrupt = StepOutcome::Interrupt {
            question: "what?".into(),
            candidates: vec!["a".into(), "b".into()],
        };
        let err = StepOutcome::Error {
            message: "boom".into(),
        };

        match cont {
            StepOutcome::Continue { .. } => {}
            _ => panic!("expected Continue"),
        }
        match done {
            StepOutcome::Done { result } => assert_eq!(result, "finished"),
            _ => panic!("expected Done"),
        }
        match interrupt {
            StepOutcome::Interrupt {
                question,
                candidates,
            } => {
                assert_eq!(question, "what?");
                assert_eq!(candidates.len(), 2);
            }
            _ => panic!("expected Interrupt"),
        }
        match err {
            StepOutcome::Error { message } => assert_eq!(message, "boom"),
            _ => panic!("expected Error"),
        }
    }

    // ── AgentEvent ──────────────────────────────────────────────────────

    #[test]
    fn test_agent_event_variants() {
        let thinking = AgentEvent::Thinking {
            content: "hmm".into(),
        };
        let tool_call = AgentEvent::ToolCall {
            name: "ls".into(),
            args: serde_json::json!({"path": "/tmp"}),
        };
        let tool_result = AgentEvent::ToolResult {
            name: "ls".into(),
            output: "file.txt".into(),
            success: true,
        };
        let response = AgentEvent::Response {
            content: "done".into(),
        };
        let safety = AgentEvent::SafetyWarning {
            message: "slow down".into(),
        };
        let done = AgentEvent::Done {
            summary: "complete".into(),
        };
        let error = AgentEvent::Error {
            message: "fail".into(),
        };

        match thinking {
            AgentEvent::Thinking { content } => assert_eq!(content, "hmm"),
            _ => panic!("expected Thinking"),
        }
        match tool_call {
            AgentEvent::ToolCall { name, .. } => assert_eq!(name, "ls"),
            _ => panic!("expected ToolCall"),
        }
        match tool_result {
            AgentEvent::ToolResult { name, success, .. } => {
                assert_eq!(name, "ls");
                assert!(success);
            }
            _ => panic!("expected ToolResult"),
        }
        match response {
            AgentEvent::Response { content } => assert_eq!(content, "done"),
            _ => panic!("expected Response"),
        }
        match safety {
            AgentEvent::SafetyWarning { message } => assert_eq!(message, "slow down"),
            _ => panic!("expected SafetyWarning"),
        }
        match done {
            AgentEvent::Done { summary } => assert_eq!(summary, "complete"),
            _ => panic!("expected Done"),
        }
        match error {
            AgentEvent::Error { message } => assert_eq!(message, "fail"),
            _ => panic!("expected Error"),
        }
    }

    // ── repair_count ────────────────────────────────────────────────────

    #[test]
    fn test_repair_count_starts_at_zero() {
        let agent = make_test_agent();
        assert_eq!(agent.repair_count(), 0);
    }

    #[test]
    fn test_repair_count_increments() {
        let mut agent = make_test_agent();
        agent.repair_count = 5;
        assert_eq!(agent.repair_count(), 5);
    }

    // ── set_event_sender ────────────────────────────────────────────────

    #[tokio::test]
    async fn test_set_event_sender() {
        let mut agent = make_test_agent();
        let (tx, _rx) = mpsc::unbounded_channel();
        agent.set_event_sender(tx);
        // Should be set; emitting should not panic
        agent.emit_event(AgentEvent::Thinking {
            content: "test".into(),
        });
        // If no crash, the test passes
    }

    #[tokio::test]
    async fn test_event_emission_without_sender_does_not_panic() {
        let agent = make_test_agent();
        // No sender set — emit_event should silently ignore
        agent.emit_event(AgentEvent::Done {
            summary: "done".into(),
        });
    }

    // ── abort ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_abort_sets_running_false() {
        let agent = make_test_agent();
        *agent.running.write().await = true;
        assert!(*agent.running.read().await);
        agent.abort().await;
        assert!(!*agent.running.read().await);
    }

    #[tokio::test]
    async fn test_abort_already_stopped() {
        let agent = make_test_agent();
        // Already false — should not panic
        agent.abort().await;
        assert!(!*agent.running.read().await);
    }

    // ── chat_session integration ────────────────────────────────────────

    #[tokio::test]
    async fn test_agent_chat_session_initialization() {
        let agent = make_test_agent();
        // chat_session starts with the system prompt message
        assert!(!agent.chat_session.messages.is_empty());
        // system prompt should contain agent name
        assert!(agent.chat_session.messages[0].content.contains("TestAgent"));
    }

    #[tokio::test]
    async fn test_agent_chat_session_clear_and_compact_flow() {
        let mut agent = make_test_agent();
        agent.chat_session.add_user_message("Message A");
        agent.chat_session.add_assistant_message("Reply A");
        agent.chat_session.add_user_message("Message B");
        // Initial messages: system prompt + 3 user/assistant messages
        assert_eq!(agent.chat_session.messages.len(), 4);

        // Compact under threshold should not change anything
        agent.compact_if_needed();
        assert_eq!(agent.chat_session.messages.len(), 4);
    }

    // ── set_skill_loader ────────────────────────────────────────────────

    #[tokio::test]
    async fn test_set_skill_loader() {
        let mut agent = make_test_agent();
        assert!(agent.skill_loader.is_none());
        let loader = soul_skills::SkillLoader::new(std::path::Path::new("/tmp/test-skills"));
        agent.set_skill_loader(loader);
        assert!(agent.skill_loader.is_some());
    }
}
