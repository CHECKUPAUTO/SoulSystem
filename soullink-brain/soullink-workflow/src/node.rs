//! Node types and execution logic.

use crate::conditions::LoopConfig;
use crate::context::WorkflowContext;
use serde::{Deserialize, Serialize};
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

        // Call the configured LLM endpoint; if none is configured the context
        // runs in echo mode (returns the assembled prompt) so workflows remain
        // runnable without a model.
        self.finish_with_llm(ctx, prompt).await
    }

    /// Run `prompt` through the context's LLM when one is configured, else echo
    /// it back. Stores the result under this node's id either way.
    async fn finish_with_llm(&self, ctx: &WorkflowContext, prompt: String) -> NodeResult {
        if ctx.has_llm() {
            match ctx.generate(&prompt).await {
                Ok(resp) => {
                    ctx.set(&self.id, &resp).await;
                    NodeResult::Success(resp)
                }
                Err(e) => NodeResult::Failed(format!("LLM call failed: {e}")),
            }
        } else {
            ctx.set(&self.id, &prompt).await;
            NodeResult::Success(prompt)
        }
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

        self.finish_with_llm(ctx, prompt).await
    }

    /// Run a workflow's `bash` node.
    ///
    /// This used to be `sh -c <command>`, which handed a workflow definition a
    /// full shell with this process's privileges. It now goes through
    /// [`soul_sandbox::Sandbox`], which refuses `sh` as a head binary and runs
    /// the command directly under seccomp, `setrlimit` and a network namespace
    /// (INV-EXEC-1).
    ///
    /// **This changes what a bash node can express.** Pipelines, redirects and
    /// `&&` chains no longer work: the sandbox neutralises them precisely
    /// because a shell is what it exists to avoid. A workflow that relied on
    /// `a | b` now fails rather than silently doing something else, which is
    /// the failure direction to prefer, but it *is* a behaviour change for
    /// existing workflow definitions rather than a transparent hardening.
    async fn execute_bash(bash: BashNode, _ctx: &WorkflowContext) -> NodeResult {
        let dur = bash
            .timeout
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(300));
        let command = bash.command.clone();

        let policy = soul_sandbox::SandboxPolicy {
            timeout: dur,
            ..Default::default()
        };

        let result = timeout(dur, async {
            tokio::task::spawn_blocking(move || {
                soul_sandbox::Sandbox::new(policy).execute(&command)
            })
            .await
        })
        .await;

        match result {
            Ok(Ok(Ok(verdict))) => {
                if verdict.exit_code == Some(bash.expected_exit_code) {
                    NodeResult::Success(verdict.stdout)
                } else {
                    NodeResult::Failed(format!(
                        "exit {:?}: {}\n{}",
                        verdict.exit_code, verdict.stdout, verdict.stderr
                    ))
                }
            }
            Ok(Ok(Err(e))) => NodeResult::Failed(format!("sandbox refused command: {e}")),
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
