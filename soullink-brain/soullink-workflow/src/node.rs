//! Node types and execution logic.

use crate::conditions::LoopConfig;
use crate::context::WorkflowContext;
use serde::{Deserialize, Serialize};
use std::process::Command;
use tokio::time::{timeout, Duration};
use tracing::info;

// ── Node types ──────────────────────────────────────────────────────────────

/// A typed node in a workflow.
#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub depends_on: Vec<String>,
    pub node_type: NodeType,
    pub loop_config: Option<LoopConfig>,
}

/// Discriminant for what kind of work a node performs.
#[derive(Debug, Clone)]
pub enum NodeType {
    Ai(AiNode),
    Agent(AgentNode),
    Bash(BashNode),
    Approval(ApprovalNode),
    Parallel(ParallelNode),
}

/// AI-prompt node: sends a prompt to the LLM (optionally enriched with HNN mesh state).
#[derive(Debug, Clone, Deserialize)]
pub struct AiNode {
    pub prompt: String,
    pub model: Option<String>,
    /// When true, prepend HNN mesh snapshot to the prompt.
    #[serde(default)]
    pub context: bool,
}

/// Deterministic bash command node.
#[derive(Debug, Clone, Deserialize)]
pub struct BashNode {
    pub command: String,
    #[serde(default = "default_exit_code", rename = "expected_exit_code")]
    pub expected_exit_code: i32,
    pub timeout: Option<u64>,
}

fn default_exit_code() -> i32 {
    0
}

/// Human approval gate — pauses execution until approved.
#[derive(Debug, Clone, Deserialize)]
pub struct ApprovalNode {
    /// Optional prompt describing what to approve.
    #[serde(default)]
    pub prompt: String,
}

/// Run multiple sub-nodes concurrently.
#[derive(Debug, Clone)]
pub struct ParallelNode {
    pub nodes: Vec<Node>,
}

/// Agent node — CrewAI-inspired role-based AI worker.
/// Unlike AiNode (raw prompt), an AgentNode has a role, goal, and backstory
/// that shape its system prompt, producing higher-quality specialized output.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentNode {
    /// Role title (e.g. "Researcher", "Writer", "Editor", "Security Auditor")
    pub role: String,
    /// What this agent is trying to accomplish
    pub goal: String,
    /// Background context that shapes the agent's perspective
    #[serde(default)]
    pub backstory: String,
    /// The task/prompt for this specific execution
    pub task: String,
    /// When true, prepend HNN mesh snapshot + memory context
    #[serde(default)]
    pub context: bool,
    /// Optional model override
    pub model: Option<String>,
}

// ── Execution result ───────────────────────────────────────────────────────

/// Result of executing a single node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeResult {
    Success(String),
    Failed(String),
    NeedsApproval(String),
    Looping { iteration: usize, output: String },
}

// ── Execution ───────────────────────────────────────────────────────────────

impl Node {
    /// Execute this node within the given workflow context.
    pub async fn execute(&self, ctx: &WorkflowContext) -> NodeResult {
        match &self.node_type {
            NodeType::Ai(ai) => self.execute_ai(ai, ctx).await,
            NodeType::Agent(agent) => self.execute_agent(agent, ctx).await,
            NodeType::Bash(bash) => Self::execute_bash(bash.clone(), ctx).await,
            NodeType::Approval(approval) => Self::execute_approval(approval.clone(), ctx).await,
            NodeType::Parallel(parallel) => Self::execute_parallel(parallel.clone(), ctx).await,
        }
    }

    async fn execute_ai(&self, ai: &AiNode, ctx: &WorkflowContext) -> NodeResult {
        let mut prompt = ai.prompt.clone();

        if ai.context {
            match ctx.fetch_mesh_snapshot().await {
                Ok(snapshot) => {
                    prompt = format!("[HNN Mesh Context]\n{}\n\n---\n\n{}", snapshot, prompt);
                }
                Err(e) => {
                    info!("Failed to fetch mesh snapshot, proceeding without: {e}");
                }
            }
        }

        // Memory enrichment: when memory feature is enabled and context is true,
        // the WorkflowEngine injects memory search results via context key "memory_context"
        #[cfg(feature = "memory")]
        if ai.context {
            if let Some(mem_ctx) = ctx.get("memory_context").await {
                prompt = format!(
                    "[Memory Context]\n{}\n[/Memory Context]\n\n{}",
                    mem_ctx, prompt
                );
            }
        }

        // In a real implementation, this would call the LLM via soullink-core's Ollama client.
        // For now, store the prompt as context and return success.
        ctx.set(&self.id, &prompt).await;
        NodeResult::Success(prompt)
    }

    /// Execute an Agent node — CrewAI-inspired role-based AI worker.
    /// Constructs a structured system prompt from role/goal/backstory,
    /// then delegates to the LLM with full mesh + memory context.
    async fn execute_agent(&self, agent: &AgentNode, ctx: &WorkflowContext) -> NodeResult {
        // Build system prompt from agent identity
        let system_prompt = format!(
            "You are a {role}.\n\nGoal: {goal}\n\nBackstory: {backstory}",
            role = agent.role,
            goal = agent.goal,
            backstory = if agent.backstory.is_empty() {
                "N/A"
            } else {
                &agent.backstory
            },
        );

        let mut prompt = format!(
            "[System]\n{}\n[/System]\n\n[Task]\n{}\n[/Task]",
            system_prompt, agent.task
        );

        // Enrich with mesh context
        if agent.context {
            match ctx.fetch_mesh_snapshot().await {
                Ok(snapshot) => {
                    prompt = format!(
                        "[HNN Mesh Context]\n{}\n[/HNN Mesh Context]\n\n{}",
                        snapshot, prompt
                    );
                }
                Err(e) => {
                    info!("Failed to fetch mesh snapshot, proceeding without: {e}");
                }
            }

            // Memory enrichment
            #[cfg(feature = "memory")]
            if let Some(mem_ctx) = ctx.get("memory_context").await {
                prompt = format!(
                    "[Memory Context]\n{}\n[/Memory Context]\n\n{}",
                    mem_ctx, prompt
                );
            }
        }

        ctx.set(&self.id, &prompt).await;
        NodeResult::Success(prompt)
    }

    async fn execute_bash(bash: BashNode, _ctx: &WorkflowContext) -> NodeResult {
        let dur = bash
            .timeout
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(300));
        let command = bash.command.clone();

        let result = timeout(dur, async {
            tokio::task::spawn_blocking(move || Command::new("sh").arg("-c").arg(&command).output())
                .await
        })
        .await;

        match result {
            Ok(Ok(Ok(output))) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                if output.status.code() == Some(bash.expected_exit_code) {
                    NodeResult::Success(stdout)
                } else {
                    NodeResult::Failed(format!(
                        "exit {:?}: {stdout}\n{stderr}",
                        output.status.code()
                    ))
                }
            }
            Ok(Ok(Err(e))) => NodeResult::Failed(format!("spawn error: {e}")),
            Ok(Err(e)) => NodeResult::Failed(format!("join error: {e}")),
            Err(_) => NodeResult::Failed("command timed out".into()),
        }
    }

    async fn execute_approval(approval: ApprovalNode, _ctx: &WorkflowContext) -> NodeResult {
        // In a real implementation, this would pause and wait for human input.
        // For library mode, we return NeedsApproval.
        NodeResult::NeedsApproval(approval.prompt.clone())
    }

    async fn execute_parallel(parallel: ParallelNode, ctx: &WorkflowContext) -> NodeResult {
        // Execute sub-nodes concurrently using join_all (no spawn needed)
        let mut futures = Vec::new();
        for node in &parallel.nodes {
            let node = node.clone();
            let ctx = ctx.clone();
            futures.push(async move { node.execute(&ctx).await });
        }
        let results = futures::future::join_all(futures).await;

        let mut has_failure = false;
        let mut combined = String::new();
        for result in &results {
            match result {
                NodeResult::Failed(e) => {
                    has_failure = true;
                    combined.push_str(e);
                    combined.push('\n');
                }
                NodeResult::Success(output) => {
                    combined.push_str(output);
                    combined.push('\n');
                }
                _ => {}
            }
        }

        if has_failure {
            NodeResult::Failed(combined)
        } else {
            NodeResult::Success(combined)
        }
    }
}
