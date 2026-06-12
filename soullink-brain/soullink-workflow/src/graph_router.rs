//! Graph Router — LLM-driven DAG generation for agent workflows.
//!
//! Pattern extracted from VideoAgent (HKUDS):
//!   1. Intent Analysis → decompose user request into intent labels
//!   2. Intent → Tools mapping (soul_intents.yml)
//!   3. Graph Generation → LLM builds execution DAG (Agent Graph + Agent Chain)
//!   4. User Input Graph → parameters requiring user input
//!
//! The LLM receives registered agent metadata + the graph design prompt,
//! and returns a structured JSON DAG that the existing WorkflowEngine executes.

use crate::context::WorkflowContext;
use crate::node::{AgentNode, Node, NodeResult, NodeType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use tracing::{info, warn};

// ── Error types ─────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum GraphRouterError {
    #[error("LLM request failed: {0}")]
    LlmError(String),
    #[error("Invalid JSON response from LLM: {0}")]
    JsonParseError(String),
    #[error("Missing required field in LLM response: {0}")]
    MissingField(String),
    #[error("Intent registry error: {0}")]
    RegistryError(String),
    #[error("No valid agent graph could be generated after {attempts} attempts")]
    ExhaustedRetries { attempts: usize },
}

// ── Intent Registry ─────────────────────────────────────────────────────────

/// Mapping from intent labels to registered agent names.
pub type IntentRegistry = HashMap<String, Vec<String>>;

/// Metadata about a registered agent, sent to the LLM for graph generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetadata {
    pub name: String,
    pub description: String,
    pub inputs: Vec<ParamSchema>,
    pub outputs: Vec<ParamSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamSchema {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String,
    pub description: String,
}

// ── LLM Output Schemas ──────────────────────────────────────────────────────

/// The complete output the LLM produces when designing an agent graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentGraphOutput {
    #[serde(rename = "Feasibility")]
    pub feasibility: String,
    #[serde(rename = "Agent Graph")]
    pub agent_graph: Vec<AgentGraphNode>,
    #[serde(rename = "Agent Chain")]
    pub agent_chain: Vec<String>,
    #[serde(rename = "User Input Graph")]
    pub user_input_graph: Vec<UserInputNode>,
    #[serde(rename = "Reasoning")]
    pub reasoning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentGraphNode {
    pub node: String,
    pub inputs: Vec<GraphParam>,
    pub outputs: Vec<GraphOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphParam {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphOutput {
    pub name: String,
    pub description: String,
    pub links: Vec<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInputNode {
    pub node: String,
    pub description: String,
    pub links: Vec<HashMap<String, String>>,
}

// ── Judge / Self-Evaluation Schemas ──────────────────────────────────────────

/// Output from the Judge phase of self-evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeOutput {
    #[serde(rename = "Result")]
    pub result: String, // "0" = valid, "1" = invalid
    #[serde(rename = "Reasoning")]
    pub reasoning: String,
}

// ── Graph Design Prompt ──────────────────────────────────────────────────────

/// The system prompt sent to the LLM for graph generation.
/// Adapted from VideoAgent's graph.txt, generalized beyond video domain.
pub const GRAPH_DESIGN_PROMPT: &str = r#"You are an Agent Graph Designer for SoulSystem.

I will provide:
1. User Requirement — what the user wants to accomplish
2. Registered Agents — metadata (name, description, inputs, outputs)

Your task is to:
1. Judge Feasibility: "Feasible" or "Infeasible"
2. Design Executable Agent Graph — nodes with inputs/outputs and links
3. Generate Agent Chain — topological execution order
4. Generate User Input Graph — parameters the user must provide
5. Output Reasoning — <200 words explaining the workflow

Agent Graph format:
[
  {
    "node": "AgentName",
    "inputs": [
      {"name": "param1", "description": "..."}
    ],
    "outputs": [
      {
        "name": "output1",
        "description": "...",
        "links": [{"NextAgent": "input_param_name"}]
      }
    ]
  }
]

Agent Chain format: ["agent1", "agent2", ...]

User Input Graph format:
[
  {
    "node": "UserInputName",
    "description": "...",
    "links": [{"AgentName": "input_param_name"}]
  }
]

Rules:
- Parameter nodes with no incoming edges = user inputs
- Output links must point to inputs that actually exist in the target agent
- No functionally redundant agents
- For vague requirements, lenient evaluation is acceptable

JSON Output Format:
{
  "Feasibility": "Feasible" or "Infeasible",
  "Agent Graph": [...],
  "Agent Chain": [...],
  "User Input Graph": [...],
  "Reasoning": "..."
}
Strictly follow JSON format!"#;

// ── Intent Analysis Prompt ──────────────────────────────────────────────────

pub const INTENT_ANALYSIS_PROMPT: &str = r#"You are an intent analyst for SoulSystem.
I will provide a set of candidate intents and a user's requirements.
Select as many relevant candidate intents as possible. Consider keywords, task types, and context.

Candidate intents:
{intents}

User requirements:
{reqs}

Output ONLY a pure Python list: ['intent1', 'intent2', ...]
No analysis, no explanation."#;

// ── Judge Prompt ─────────────────────────────────────────────────────────────

pub const JUDGE_PROMPT: &str = r#"You are an agent graph validation system.
Evaluate whether the candidate agent graph correctly fulfills the user requirements.

User Requirements:
{reqs}

Registered agent metadata:
{tools}

Candidate Agent Graph:
{agent_graph}

Agent Chain:
{agent_chain}

Required User Inputs:
{user_inputs}

Evaluation Criteria:
1. Execution sequence of agents is correct
2. Parameter nodes with no incoming edges are correctly classified as user inputs
3. Output parameters are correctly routed to intended agent inputs
4. No functionally redundant agents
5. For vaguely mentioned requirements, lenient evaluation is acceptable

Output ONLY pure JSON:
{"Result": "0" if correct else "1", "Reasoning": "<100 words>"}"#;

// ── Reflection Prompt ───────────────────────────────────────────────────────

pub const REFLECTION_PROMPT: &str = r#"You are an agent graph reflection system.
Reflect on the previous validation result and identify overlooked issues.

User Requirements:
{reqs}

Registered agent metadata:
{tools}

Candidate Agent Graph:
{agent_graph}

Agent Chain:
{agent_chain}

Previous validation result:
{judge_result}

Reflection Task:
1. If validation was '0' (pass), reflect on potentially overlooked aspects
2. If validation was '1' (fail), confirm the reasoning was correct
3. Provide actionable feedback to improve the graph

Output ONLY pure JSON:
{"Result": "0" if the original validation stands, "1" if it should be overturned, "Reasoning": "<100 words>"}"#;

// ── Graph Router ────────────────────────────────────────────────────────────

/// The Graph Router transforms a user request into an executable agent DAG.
///
/// Process:
///   1. Intent Analysis — classify the request into intent labels
///   2. Intent → Tools — map intents to registered agent metadata
///   3. Graph Generation — LLM designs the execution DAG
///   4. Self-Evaluation — validate + reflect (with retries)
pub struct GraphRouter {
    pub intent_registry: IntentRegistry,
    pub agent_metadata: Vec<AgentMetadata>,
    /// Ollama API base URL for LLM calls
    pub ollama_url: String,
    /// Model to use for graph generation
    pub model: String,
    /// Maximum retries for graph generation
    pub max_retries: usize,
}

impl GraphRouter {
    pub fn new(ollama_url: &str, model: &str) -> Self {
        Self {
            intent_registry: HashMap::new(),
            agent_metadata: Vec::new(),
            ollama_url: ollama_url.to_string(),
            model: model.to_string(),
            max_retries: 3,
        }
    }

    /// Load intent registry from YAML.
    pub fn load_intents(&mut self, path: &std::path::Path) -> Result<(), GraphRouterError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            GraphRouterError::RegistryError(format!("Failed to read intents file: {e}"))
        })?;

        self.intent_registry = serde_yaml::from_str(&content).map_err(|e| {
            GraphRouterError::RegistryError(format!("Failed to parse intents YAML: {e}"))
        })?;

        info!("Loaded {} intent categories", self.intent_registry.len());
        Ok(())
    }

    /// Register agent metadata for LLM context.
    pub fn register_agent(&mut self, metadata: AgentMetadata) {
        self.agent_metadata.push(metadata);
    }

    /// Analyze user request → intent labels via LLM.
    pub async fn analyze_intents(
        &self,
        requirement: &str,
    ) -> Result<Vec<String>, GraphRouterError> {
        let intents: Vec<String> = self.intent_registry.keys().cloned().collect();
        let intents_str = intents.join("\n- ");

        let prompt = INTENT_ANALYSIS_PROMPT
            .replace("{intents}", &format!("- {}", intents_str))
            .replace("{reqs}", requirement);

        let response = self.call_llm(&prompt).await?;
        let parsed = self.parse_list_from_llm(&response)?;
        info!("Intent analysis → {:?}", parsed);
        Ok(parsed)
    }

    /// Map intent labels → agent metadata.
    pub fn get_tools_for_intents(
        &self,
        intent_list: &[String],
    ) -> Result<Vec<AgentMetadata>, GraphRouterError> {
        let mut tool_names = Vec::new();
        for intent in intent_list {
            if let Some(agents) = self.intent_registry.get(intent) {
                tool_names.extend(agents.clone());
            }
        }

        // Deduplicate
        tool_names.sort();
        tool_names.dedup();

        let mut tools = Vec::new();
        for name in &tool_names {
            if let Some(meta) = self.agent_metadata.iter().find(|m| &m.name == name) {
                tools.push(meta.clone());
            }
        }

        if tools.is_empty() {
            return Err(GraphRouterError::RegistryError(
                "No matching agents found for intents".into(),
            ));
        }

        Ok(tools)
    }

    /// Generate an agent graph from the user requirement.
    /// Includes intent analysis + graph generation + optional self-evaluation.
    pub async fn generate_graph(
        &self,
        requirement: &str,
    ) -> Result<AgentGraphOutput, GraphRouterError> {
        // Step 1: Intent Analysis
        let intents = self.analyze_intents(requirement).await?;

        // Step 2: Intent → Tools
        let tools = self.get_tools_for_intents(&intents)?;
        let tools_json = serde_json::to_string_pretty(&tools).map_err(|e| {
            GraphRouterError::JsonParseError(format!("Failed to serialize tools: {e}"))
        })?;

        // Step 3: Graph Generation (with retries + self-evaluation)
        for attempt in 0..self.max_retries {
            let graph = self.try_generate_graph(requirement, &tools_json).await?;

            // If infeasible, reflect on intents
            if graph.feasibility == "Infeasible" {
                warn!(
                    "Graph infeasible (attempt {}/{}): {}",
                    attempt + 1,
                    self.max_retries,
                    graph.reasoning
                );
                continue;
            }

            // Self-evaluation
            match self.evaluate_graph(requirement, &tools_json, &graph).await {
                Ok(true) => {
                    info!("Graph validated — feasibility: {}", graph.feasibility);
                    return Ok(graph);
                }
                Ok(false) => {
                    warn!(
                        "Graph validation failed (attempt {}/{}), regenerating...",
                        attempt + 1,
                        self.max_retries
                    );
                    continue;
                }
                Err(e) => {
                    warn!("Self-evaluation error: {e}, accepting graph optimistically");
                    return Ok(graph);
                }
            }
        }

        Err(GraphRouterError::ExhaustedRetries {
            attempts: self.max_retries,
        })
    }

    /// Single graph generation attempt.
    async fn try_generate_graph(
        &self,
        requirement: &str,
        tools_json: &str,
    ) -> Result<AgentGraphOutput, GraphRouterError> {
        let prompt = format!(
            "{}\n\nUser requirements:\n{}\n\nMetadata of registered agents:\n{}",
            GRAPH_DESIGN_PROMPT, requirement, tools_json
        );

        let response = self.call_llm(&prompt).await?;
        let graph = self.parse_graph_from_llm(&response)?;

        // Validate required fields
        if graph.agent_graph.is_empty() || graph.agent_chain.is_empty() {
            return Err(GraphRouterError::MissingField(
                "Empty agent graph or chain".into(),
            ));
        }

        Ok(graph)
    }

    /// Self-evaluation: Judge → Reflect.
    /// Returns true if the graph passes validation.
    async fn evaluate_graph(
        &self,
        requirement: &str,
        tools_json: &str,
        graph: &AgentGraphOutput,
    ) -> Result<bool, GraphRouterError> {
        let agent_graph_json = serde_json::to_string_pretty(&graph.agent_graph).unwrap_or_default();
        let agent_chain_json = serde_json::to_string_pretty(&graph.agent_chain).unwrap_or_default();
        let user_input_json =
            serde_json::to_string_pretty(&graph.user_input_graph).unwrap_or_default();

        // Phase 1: Judge
        let judge_prompt = JUDGE_PROMPT
            .replace("{reqs}", requirement)
            .replace("{tools}", tools_json)
            .replace("{agent_graph}", &agent_graph_json)
            .replace("{agent_chain}", &agent_chain_json)
            .replace("{user_inputs}", &user_input_json);

        let judge_response = self.call_llm(&judge_prompt).await?;
        let judge: JudgeOutput = self.parse_json_from_llm(&judge_response)?;

        if judge.result == "0" {
            // Phase 2: Reflect (even if passed, sanity check)
            let reflection_prompt = REFLECTION_PROMPT
                .replace("{reqs}", requirement)
                .replace("{tools}", tools_json)
                .replace("{agent_graph}", &agent_graph_json)
                .replace("{agent_chain}", &agent_chain_json)
                .replace(
                    "{judge_result}",
                    &serde_json::to_string(&judge).unwrap_or_default(),
                );

            let reflection_response = self.call_llm(&reflection_prompt).await?;
            let reflection: JudgeOutput = self.parse_json_from_llm(&reflection_response)?;

            Ok(reflection.result == "0")
        } else {
            Ok(false)
        }
    }

    /// Convert a generated AgentGraphOutput into executable Nodes for the WorkflowEngine.
    pub fn graph_to_nodes(graph: &AgentGraphOutput) -> Vec<Node> {
        let mut nodes = Vec::new();

        for graph_node in &graph.agent_graph {
            let mut depends_on = Vec::new();
            let mut inputs_desc = String::new();

            for input in &graph_node.inputs {
                inputs_desc.push_str(&format!("- {}: {}\n", input.name, input.description));
            }

            let agent = AgentNode {
                role: graph_node.node.clone(),
                goal: format!("Execute {} node", graph_node.node),
                backstory: format!("Agent inputs:\n{}", inputs_desc),
                task: format!(
                    "Process inputs and produce outputs: {:?}",
                    graph_node
                        .outputs
                        .iter()
                        .map(|o| o.name.clone())
                        .collect::<Vec<_>>()
                ),
                context: true,
                model: None,
            };

            // Build dependency edges from the graph links
            // Any agent whose output links TO this node is a dependency
            for other_node in &graph.agent_graph {
                for output in &other_node.outputs {
                    for link in &output.links {
                        if link
                            .values()
                            .any(|v| graph_node.inputs.iter().any(|i| i.name == *v))
                        {
                            if !depends_on.contains(&other_node.node) {
                                depends_on.push(other_node.node.clone());
                            }
                        }
                    }
                }
            }

            nodes.push(Node {
                id: graph_node.node.clone(),
                depends_on,
                node_type: NodeType::Agent(agent),
                loop_config: None,
            });
        }

        nodes
    }

    // ── LLM Helpers ────────────────────────────────────────────────────────

    async fn call_llm(&self, prompt: &str) -> Result<String, GraphRouterError> {
        let client = reqwest::Client::new();
        let body = serde_json::json!({
            "model": self.model,
            "prompt": prompt,
            "stream": false,
        });

        let resp = client
            .post(format!("{}/api/generate", self.ollama_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| GraphRouterError::LlmError(format!("Ollama request failed: {e}")))?;

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| GraphRouterError::LlmError(format!("Ollama response parse: {e}")))?;

        json["response"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| {
                GraphRouterError::LlmError("No 'response' field in Ollama output".into())
            })
    }

    fn parse_list_from_llm(&self, raw: &str) -> Result<Vec<String>, GraphRouterError> {
        // Extract [...] from LLM output
        let start = raw
            .find('[')
            .ok_or_else(|| GraphRouterError::JsonParseError("No '[' found in LLM output".into()))?;
        let end = raw
            .rfind(']')
            .ok_or_else(|| GraphRouterError::JsonParseError("No ']' found in LLM output".into()))?;

        let list_str = &raw[start..=end];
        // Python-like list → JSON array (replace single quotes with double)
        let json_str = list_str.replace('\'', "\"");

        serde_json::from_str(&json_str)
            .map_err(|e| GraphRouterError::JsonParseError(format!("Failed to parse list: {e}")))
    }

    fn parse_json_from_llm<T: for<'de> Deserialize<'de>>(
        &self,
        raw: &str,
    ) -> Result<T, GraphRouterError> {
        let start = raw
            .find('{')
            .ok_or_else(|| GraphRouterError::JsonParseError("No '{' found in LLM output".into()))?;
        let end = raw
            .rfind('}')
            .ok_or_else(|| GraphRouterError::JsonParseError("No '}' found in LLM output".into()))?;

        let json_str = &raw[start..=end];
        serde_json::from_str(json_str)
            .map_err(|e| GraphRouterError::JsonParseError(format!("Failed to parse JSON: {e}")))
    }

    fn parse_graph_from_llm(&self, raw: &str) -> Result<AgentGraphOutput, GraphRouterError> {
        // Find the outermost JSON object (nested braces for Agent Graph array)
        let start = raw
            .find("{\n  \"Feasibility\"")
            .or_else(|| raw.find("{\"Feasibility\""))
            .or_else(|| raw.find('{'))
            .ok_or_else(|| {
                GraphRouterError::JsonParseError("No AgentGraphOutput JSON found".into())
            })?;

        // Use brace counting to find the matching closing brace
        let mut depth = 0;
        let mut end = start;
        for (i, ch) in raw[start..].char_indices() {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    end = start + i;
                    break;
                }
            }
        }

        let json_str = &raw[start..=end];
        serde_json::from_str(json_str).map_err(|e| {
            GraphRouterError::JsonParseError(format!(
                "Failed to parse AgentGraphOutput: {e}\nJSON: {}",
                &json_str[..json_str.len().min(200)]
            ))
        })
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_list_from_llm() {
        let router = GraphRouter::new("http://localhost:11434", "deepseek-v4-flash:cloud");
        let result = router
            .parse_list_from_llm("Selected intents: ['code_review', 'debugging']")
            .unwrap();
        assert_eq!(result, vec!["code_review", "debugging"]);
    }

    #[test]
    fn test_parse_graph_from_llm() {
        let router = GraphRouter::new("http://localhost:11434", "deepseek-v4-flash:cloud");
        let json = r#"{
  "Feasibility": "Feasible",
  "Agent Graph": [
    {
      "node": "code_reviewer",
      "inputs": [{"name": "repo_path", "description": "Path to repository"}],
      "outputs": [{"name": "report", "description": "Review report", "links": [{"lint_checker": "review_report"}]}]
    },
    {
      "node": "lint_checker",
      "inputs": [{"name": "review_report", "description": "Report from code reviewer"}],
      "outputs": [{"name": "lint_results", "description": "Lint violations", "links": []}]
    }
  ],
  "Agent Chain": ["code_reviewer", "lint_checker"],
  "User Input Graph": [
    {"node": "repo_path", "description": "Path to the repository", "links": [{"code_reviewer": "repo_path"}]}
  ],
  "Reasoning": "Two-step pipeline: review then lint, feeding the review report into the linter."
}"#;

        let graph = router.parse_graph_from_llm(json).unwrap();
        assert_eq!(graph.feasibility, "Feasible");
        assert_eq!(graph.agent_graph.len(), 2);
        assert_eq!(graph.agent_chain, vec!["code_reviewer", "lint_checker"]);
        assert_eq!(graph.user_input_graph.len(), 1);
    }

    #[test]
    fn test_graph_to_nodes() {
        let graph = AgentGraphOutput {
            feasibility: "Feasible".into(),
            agent_graph: vec![
                AgentGraphNode {
                    node: "code_reviewer".into(),
                    inputs: vec![GraphParam {
                        name: "repo_path".into(),
                        description: "Path".into(),
                    }],
                    outputs: vec![GraphOutput {
                        name: "report".into(),
                        description: "Report".into(),
                        links: vec![{
                            let mut m = HashMap::new();
                            m.insert("lint_checker".into(), "review_report".into());
                            m
                        }],
                    }],
                },
                AgentGraphNode {
                    node: "lint_checker".into(),
                    inputs: vec![GraphParam {
                        name: "review_report".into(),
                        description: "Report".into(),
                    }],
                    outputs: vec![GraphOutput {
                        name: "lint_results".into(),
                        description: "Lint".into(),
                        links: vec![],
                    }],
                },
            ],
            agent_chain: vec!["code_reviewer".into(), "lint_checker".into()],
            user_input_graph: vec![UserInputNode {
                node: "repo_path".into(),
                description: "Path".into(),
                links: vec![{
                    let mut m = HashMap::new();
                    m.insert("code_reviewer".into(), "repo_path".into());
                    m
                }],
            }],
            reasoning: "Two-step".into(),
        };

        let nodes = GraphRouter::graph_to_nodes(&graph);
        assert_eq!(nodes.len(), 2);
        assert!(nodes.iter().any(|n| n.id == "code_reviewer"));
        assert!(nodes.iter().any(|n| n.id == "lint_checker"));
        // lint_checker depends on code_reviewer
        let lint = nodes.iter().find(|n| n.id == "lint_checker").unwrap();
        assert!(lint.depends_on.contains(&"code_reviewer".to_string()));
    }
}
