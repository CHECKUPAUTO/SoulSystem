//! Workflow execution engine: DAG building, topological sort, loop handling.
//!
//! Two modes:
//!   - Static: YAML-defined workflows loaded via `load_workflow()`
//!   - Dynamic: LLM-generated workflows via `run_dynamic()` (VideoAgent pattern)

use crate::agent_registry::AgentRegistry;
use crate::config::WorkflowConfig;
use crate::context::WorkflowContext;
use crate::git_worktree::WorktreeManager;
use crate::graph_router::GraphRouter;
use crate::node::{Node, NodeResult, NodeType};
use petgraph::algo::toposort;
use petgraph::graph::DiGraph;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tracing::{info, warn};

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("workflow not found: {0}")]
    NotFound(String),
    #[error("config error: {0}")]
    Config(#[from] crate::config::ConfigError),
    #[error("cycle detected in workflow DAG")]
    Cycle,
    #[error("node execution failed: {0}")]
    NodeFailed(String),
    #[error("approval required: {0}")]
    NeedsApproval(String),
}

/// Result of running an entire workflow.
#[derive(Debug)]
pub struct WorkflowResult {
    pub workflow_name: String,
    pub outputs: HashMap<String, String>,
    pub status: WorkflowStatus,
}

#[derive(Debug, PartialEq)]
pub enum WorkflowStatus {
    Success,
    Failed(String),
    NeedsApproval(String),
}

/// The workflow execution engine.
pub struct WorkflowEngine {
    configs: HashMap<String, WorkflowConfig>,
    worktree: Option<WorktreeManager>,
    /// Optional memory graph for context-enriched AI node execution
    #[cfg(feature = "memory")]
    memory: Option<std::sync::Arc<soullink_memory::MemoryGraph>>,
    /// Agent registry for dynamic workflow execution
    agent_registry: Arc<AgentRegistry>,
    /// GraphRouter for LLM-driven dynamic DAG generation
    graph_router: Option<Arc<GraphRouter>>,
}

impl WorkflowEngine {
    /// Create a new engine with an empty config set.
    pub fn new() -> Self {
        Self {
            configs: HashMap::new(),
            worktree: None,
            #[cfg(feature = "memory")]
            memory: None,
            agent_registry: Arc::new(AgentRegistry::new()),
            graph_router: None,
        }
    }

    /// Attach a MemoryGraph for context-enriched AI node execution.
    #[cfg(feature = "memory")]
    pub fn with_memory(&mut self, graph: std::sync::Arc<soullink_memory::MemoryGraph>) {
        self.memory = Some(graph);
    }

    /// Set the GraphRouter for dynamic LLM-driven workflow generation.
    /// Required for `run_dynamic()` to work.
    pub fn set_graph_router(&mut self, router: GraphRouter) {
        self.graph_router = Some(Arc::new(router));
    }

    /// Get a reference to the AgentRegistry for registering custom agents.
    pub fn agent_registry(&self) -> &Arc<AgentRegistry> {
        &self.agent_registry
    }

    /// Run a workflow dynamically from a natural language request.
    /// Uses the VideoAgent pattern: Intent Analysis → Graph Generation → Execution.
    ///
    /// Requires `set_graph_router()` to have been called.
    pub async fn run_dynamic(&self, request: &str) -> Result<WorkflowResult, EngineError> {
        let router = self
            .graph_router
            .as_ref()
            .ok_or_else(|| EngineError::NodeFailed(
                "GraphRouter not configured. Call set_graph_router() first.".into()
            ))?;

        info!("Dynamic workflow request: {}", request);

        // Step 1: Generate agent graph from natural language request
        let graph_output = router
            .generate_graph(request)
            .await
            .map_err(|e| EngineError::NodeFailed(format!("Graph generation failed: {e}")))?;

        info!(
            "Generated graph: {} agents, chain: {:?}",
            graph_output.agent_graph.len(),
            graph_output.agent_chain
        );

        // Step 2: Convert graph to executable nodes
        let nodes = GraphRouter::graph_to_nodes(&graph_output);

        // Step 3: Check if nodes reference registered agents
        for node in &nodes {
            if !self.agent_registry.has(&node.id) {
                warn!("Agent '{}' not in registry — execution will be best-effort", node.id);
            }
        }

        // Step 4: Build DAG and execute
        let order = Self::build_dag(&nodes)?;
        let ctx = WorkflowContext::new(request);
        let mut outputs: HashMap<String, String> = HashMap::new();

        for idx in &order {
            let node = &nodes[*idx];

            // Check dependency failures
            for dep in &node.depends_on {
                if let Some(err) = outputs.get(dep) {
                    if err.starts_with("ERROR:") {
                        let err_msg = format!("dependency {dep} failed: {err}");
                        return Ok(WorkflowResult {
                            workflow_name: "dynamic".into(),
                            outputs: outputs.clone(),
                            status: WorkflowStatus::Failed(err_msg),
                        });
                    }
                }
            }

            // Execute: try AgentRegistry first, fall back to Node::execute
            let result = if let NodeType::Agent(ref agent_node) = node.node_type {
                self.execute_registered_agent(&node.id, agent_node, &ctx, &outputs).await
            } else {
                node.execute(&ctx).await
            };

            match result {
                NodeResult::Success(output) => {
                    ctx.set(&node.id, &output).await;
                    outputs.insert(node.id.clone(), output);
                }
                NodeResult::Failed(err) => {
                    let err_msg = format!("ERROR: {err}");
                    ctx.set(&node.id, &err_msg).await;
                    outputs.insert(node.id.clone(), err_msg);
                }
                other => {
                    info!("Node {} returned: {:?}", node.id, other);
                    if let NodeResult::Success(ref output) = other {
                        ctx.set(&node.id, output).await;
                        outputs.insert(node.id.clone(), output.clone());
                    }
                }
            }
        }

        let has_failure = outputs.values().any(|v| v.starts_with("ERROR:"));
        Ok(WorkflowResult {
            workflow_name: "dynamic".into(),
            outputs,
            status: if has_failure {
                WorkflowStatus::Failed("one or more nodes failed".into())
            } else {
                WorkflowStatus::Success
            },
        })
    }

    /// Execute a node using the AgentRegistry, with data flow from previous outputs.
    async fn execute_registered_agent(
        &self,
        node_id: &str,
        agent_node: &crate::node::AgentNode,
        ctx: &WorkflowContext,
        prev_outputs: &HashMap<String, String>,
    ) -> NodeResult {
        // Build input map from context and previous outputs
        let mut inputs = HashMap::new();

        // Try to extract inputs from the workflow context
        for (key, value) in prev_outputs {
            // Heuristic: if a previous output key matches an input param name for this agent
            inputs.entry(key.clone()).or_insert_with(|| value.clone());
        }

        // Always provide the task as content input
        inputs.entry("content".into()).or_insert_with(|| agent_node.task.clone());

        // Execute via agent registry
        match self.agent_registry.execute(node_id, &inputs) {
            Some(result) => {
                if result.success {
                    // Serialize outputs into a single string for the workflow context
                    let output_str = result
                        .outputs
                        .iter()
                        .map(|(k, v)| format!("## {}

{}", k, v))
                        .collect::<Vec<_>>()
                        .join("\n\n");
                    NodeResult::Success(output_str)
                } else {
                    NodeResult::Failed(
                        result.error.unwrap_or_else(|| "Agent execution failed".into()),
                    )
                }
            }
            None => {
                warn!("Agent '{}' not in registry, using LLM fallback", node_id);
                // Fall back to LLM-based execution
                Node {
                    id: node_id.to_string(),
                    depends_on: vec![],
                    node_type: NodeType::Agent(agent_node.clone()),
                    loop_config: None,
                }
                .execute(ctx)
                .await
            }
        }
    }

    /// Load a workflow config from a YAML file and register it.
    pub fn load_workflow(&mut self, path: &std::path::Path) -> Result<(), EngineError> {
        let config = WorkflowConfig::from_file(path)?;
        let name = config.name.clone();
        self.configs.insert(name, config);
        Ok(())
    }

    /// Register a workflow config directly.
    pub fn register_workflow(&mut self, config: WorkflowConfig) {
        let name = config.name.clone();
        self.configs.insert(name, config);
    }

    /// Set the git worktree manager for isolated runs.
    pub fn set_worktree_manager(&mut self, manager: WorktreeManager) {
        self.worktree = Some(manager);
    }

    /// Build a DAG from node dependencies and return topologically sorted node IDs.
    fn build_dag(nodes: &[Node]) -> Result<Vec<usize>, EngineError> {
        let mut graph = DiGraph::<usize, ()>::new();
        let mut idx_map: HashMap<String, petgraph::graph::NodeIndex> = HashMap::new();

        // Add nodes
        for (i, node) in nodes.iter().enumerate() {
            let idx = graph.add_node(i);
            idx_map.insert(node.id.clone(), idx);
        }

        // Add edges (dependency -> dependent)
        for node in nodes.iter() {
            let dep_idx = idx_map[&node.id];
            for dep in &node.depends_on {
                let src = idx_map
                    .get(dep)
                    .ok_or_else(|| EngineError::NodeFailed(dep.clone()))?;
                graph.add_edge(*src, dep_idx, ());
            }
        }

        toposort(&graph, None)
            .map(|sorted| sorted.iter().map(|idx| graph[*idx]).collect())
            .map_err(|_| EngineError::Cycle)
    }

    /// Run a workflow by name with the given input.
    pub async fn run(
        &self,
        workflow_name: &str,
        input: &str,
    ) -> Result<WorkflowResult, EngineError> {
        let config = self
            .configs
            .get(workflow_name)
            .ok_or_else(|| EngineError::NotFound(workflow_name.to_string()))?;

        let nodes = config.build_nodes()?;
        let order = Self::build_dag(&nodes)?;

        let ctx = WorkflowContext::new(input);

        // Inject memory context if available (feature-gated)
        #[cfg(feature = "memory")]
        if let Some(ref memory) = self.memory {
            // Search memory for relevant concepts using batch_search
            // to saturate all 64 threads on dual-Xeon
            let query_vec = vec![input.as_bytes().to_vec()]; // placeholder embedding
            if let Ok(results) = memory.search(&query_vec, 5) {
                if !results.is_empty() {
                    let mem_str: Vec<String> = results
                        .iter()
                        .map(|r| format!("{} (score: {:.3})", r.id, r.score))
                        .collect();
                    ctx.set("memory_context", &mem_str.join("\n")).await;
                }
            }
        }
        let mut outputs: HashMap<String, String> = HashMap::new();
        let mut approval_needed: Option<String> = None;

        for idx in &order {
            let node = &nodes[*idx];

            // Check if any dependency failed
            for dep in &node.depends_on {
                if let Some(err) = outputs.get(dep) {
                    if err.starts_with("ERROR:") {
                        let msg = format!("dependency {dep} failed: {err}");
                        return Ok(WorkflowResult {
                            workflow_name: workflow_name.to_string(),
                            outputs,
                            status: WorkflowStatus::Failed(msg),
                        });
                    }
                }
            }

            // Execute with loop handling
            let result = self.execute_with_loop(node, &ctx).await;

            match result {
                NodeResult::Success(output) => {
                    ctx.set(&node.id, &output).await;
                    outputs.insert(node.id.clone(), output);
                }
                NodeResult::Failed(err) => {
                    let err_msg = format!("ERROR: {err}");
                    ctx.set(&node.id, &err_msg).await;
                    outputs.insert(node.id.clone(), err_msg);
                    // Continue to allow downstream nodes to check error context
                }
                NodeResult::NeedsApproval(info) => {
                    approval_needed = Some(info);
                    break;
                }
                NodeResult::Looping { output, .. } => {
                    ctx.set(&node.id, &output).await;
                    outputs.insert(node.id.clone(), output);
                }
            }
        }

        if let Some(info) = approval_needed {
            Ok(WorkflowResult {
                workflow_name: workflow_name.to_string(),
                outputs,
                status: WorkflowStatus::NeedsApproval(info),
            })
        } else {
            let has_failure = outputs.values().any(|v| v.starts_with("ERROR:"));
            Ok(WorkflowResult {
                workflow_name: workflow_name.to_string(),
                outputs,
                status: if has_failure {
                    WorkflowStatus::Failed("one or more nodes failed".into())
                } else {
                    WorkflowStatus::Success
                },
            })
        }
    }

    /// Execute a node, handling loop config if present.
    async fn execute_with_loop(&self, node: &Node, ctx: &WorkflowContext) -> NodeResult {
        let loop_config = match &node.loop_config {
            Some(lc) => lc,
            None => return node.execute(ctx).await,
        };

        let max_iter = loop_config.max_iterations;
        let mut iteration = 0;

        loop {
            iteration += 1;
            info!("Loop node {} iteration {iteration}/{max_iter}", node.id);

            let result = node.execute(ctx).await;

            // Check exit condition
            if loop_config.condition.is_satisfied(iteration) {
                match result {
                    NodeResult::Success(output) => return NodeResult::Success(output),
                    _ => return result,
                }
            }

            // Check max iterations
            if iteration >= max_iter {
                warn!("Loop node {} hit max iterations ({max_iter})", node.id);
                match result {
                    NodeResult::Success(output) | NodeResult::Looping { output, .. } => {
                        return NodeResult::Success(output);
                    }
                    other => return other,
                }
            }

            // Store intermediate result
            if let NodeResult::Success(ref output) | NodeResult::Looping { ref output, .. } = result
            {
                ctx.set(&node.id, output).await;
            }
        }
    }
}

impl Default for WorkflowEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WorkflowConfig;

    #[test]
    fn build_dag_linear() {
        let yaml = r#"
name: linear
description: "linear workflow"
nodes:
  - id: a
    prompt: "step a"
  - id: b
    depends_on: [a]
    prompt: "step b"
  - id: c
    depends_on: [b]
    prompt: "step c"
"#;
        let config = WorkflowConfig::from_str(yaml).unwrap();
        let nodes = config.build_nodes().unwrap();
        let order = WorkflowEngine::build_dag(&nodes).unwrap();

        let ids: Vec<&str> = order.iter().map(|i| nodes[*i].id.as_str()).collect();
        assert!(ids.iter().position(|&x| x == "a") < ids.iter().position(|&x| x == "b"));
        assert!(ids.iter().position(|&x| x == "b") < ids.iter().position(|&x| x == "c"));
    }

    #[test]
    fn build_dag_diamond() {
        let yaml = r#"
name: diamond
description: "diamond workflow"
nodes:
  - id: start
    prompt: "go"
  - id: left
    depends_on: [start]
    prompt: "left"
  - id: right
    depends_on: [start]
    prompt: "right"
  - id: end
    depends_on: [left, right]
    prompt: "merge"
"#;
        let config = WorkflowConfig::from_str(yaml).unwrap();
        let nodes = config.build_nodes().unwrap();
        let order = WorkflowEngine::build_dag(&nodes).unwrap();

        let ids: Vec<&str> = order.iter().map(|i| nodes[*i].id.as_str()).collect();
        let start_pos = ids.iter().position(|&x| x == "start").unwrap();
        let left_pos = ids.iter().position(|&x| x == "left").unwrap();
        let right_pos = ids.iter().position(|&x| x == "right").unwrap();
        let end_pos = ids.iter().position(|&x| x == "end").unwrap();
        assert!(start_pos < left_pos);
        assert!(start_pos < right_pos);
        assert!(left_pos < end_pos);
        assert!(right_pos < end_pos);
    }

    #[tokio::test]
    async fn run_simple_workflow() {
        let yaml = r#"
name: simple
description: "simple test"
nodes:
  - id: step1
    prompt: "do something"
  - id: step2
    depends_on: [step1]
    bash: "echo hello"
"#;
        let config = WorkflowConfig::from_str(yaml).unwrap();
        let mut engine = WorkflowEngine::new();
        engine.register_workflow(config);
        let result = engine.run("simple", "test input").await.unwrap();
        assert!(matches!(result.status, WorkflowStatus::Success));
    }

    #[tokio::test]
    async fn run_dynamic_workflow() {
        use crate::agent_registry::AgentRegistry;
        use crate::graph_router::{AgentGraphOutput, AgentGraphNode, GraphOutput, GraphParam, UserInputNode};
        use crate::node::NodeType;

        // Build a static graph that mimics what the LLM would generate
        let graph = AgentGraphOutput {
            feasibility: "Feasible".into(),
            agent_graph: vec![
                AgentGraphNode {
                    node: "code_reviewer".into(),
                    inputs: vec![
                        GraphParam { name: "repo_path".into(), description: "Path".into() },
                        GraphParam { name: "focus_areas".into(), description: "Areas".into() },
                    ],
                    outputs: vec![
                        GraphOutput {
                            name: "report".into(),
                            description: "Code review report".into(),
                            links: vec![{
                                let mut m = std::collections::HashMap::new();
                                m.insert("lint_checker".into(), "review_report".into());
                                m
                            }],
                        },
                    ],
                },
                AgentGraphNode {
                    node: "lint_checker".into(),
                    inputs: vec![
                        GraphParam { name: "repo_path".into(), description: "Path".into() },
                        GraphParam { name: "review_report".into(), description: "Review".into() },
                    ],
                    outputs: vec![
                        GraphOutput {
                            name: "lint_results".into(),
                            description: "Lint report".into(),
                            links: vec![
                                {
                                    let mut m = std::collections::HashMap::new();
                                    m.insert("security_auditor".into(), "lint_results".into());
                                    m
                                },
                            ],
                        },
                    ],
                },
                AgentGraphNode {
                    node: "security_auditor".into(),
                    inputs: vec![
                        GraphParam { name: "repo_path".into(), description: "Path".into() },
                        GraphParam { name: "lint_results".into(), description: "Lint".into() },
                    ],
                    outputs: vec![
                        GraphOutput {
                            name: "audit_report".into(),
                            description: "Security report".into(),
                            links: vec![],
                        },
                    ],
                },
            ],
            agent_chain: vec!["code_reviewer".into(), "lint_checker".into(), "security_auditor".into()],
            user_input_graph: vec![
                UserInputNode {
                    node: "repo_path".into(),
                    description: "Repository path".into(),
                    links: vec![{
                        let mut m = std::collections::HashMap::new();
                        m.insert("code_reviewer".into(), "repo_path".into());
                        m.insert("lint_checker".into(), "repo_path".into());
                        m.insert("security_auditor".into(), "repo_path".into());
                        m
                    }],
                },
            ],
            reasoning: "Three-step pipeline: review → lint → audit".into(),
        };

        // Convert to nodes
        let nodes = GraphRouter::graph_to_nodes(&graph);
        assert_eq!(nodes.len(), 3);

        // Execute via engine
        let engine = WorkflowEngine::new();
        let order = WorkflowEngine::build_dag(&nodes).unwrap();

        let ctx = WorkflowContext::new("Review /tmp/test");
        let mut outputs: HashMap<String, String> = HashMap::new();

        for idx in &order {
            let node = &nodes[*idx];
            for dep in &node.depends_on {
                if let Some(err) = outputs.get(dep) {
                    if err.starts_with("ERROR:") {
                        break;
                    }
                }
            }

            // Execute via agent registry
            if let NodeType::Agent(ref agent_node) = node.node_type {
                let mut inputs = HashMap::new();
                for (k, v) in &outputs {
                    inputs.entry(k.clone()).or_insert_with(|| v.clone());
                }
                inputs.entry("repo_path".into()).or_insert_with(|| "/tmp/test".into());
                inputs.entry("content".into()).or_insert_with(|| agent_node.task.clone());

                match engine.agent_registry.execute(&node.id, &inputs) {
                    Some(result) if result.success => {
                        let out = result.outputs.values().cloned().collect::<Vec<_>>().join("\n");
                        outputs.insert(node.id.clone(), out);
                    }
                    _ => {
                        outputs.insert(node.id.clone(), format!("ERROR: agent {}", node.id));
                    }
                }
            }
        }

        assert!(outputs.contains_key("code_reviewer"));
        assert!(outputs.contains_key("lint_checker"));
        assert!(outputs.contains_key("security_auditor"));
        assert!(!outputs["code_reviewer"].starts_with("ERROR:"));
        assert!(!outputs["lint_checker"].starts_with("ERROR:"));
        assert!(!outputs["security_auditor"].starts_with("ERROR:"));
    }
}
