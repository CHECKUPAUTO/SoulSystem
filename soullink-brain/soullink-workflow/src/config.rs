//! YAML workflow configuration parser (Archon-inspired).

use crate::conditions::LoopConfig;
use crate::node::{AgentNode, AiNode, ApprovalNode, BashNode, NodeType, ParallelNode};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("duplicate node id: {0}")]
    DuplicateNodeId(String),
    #[error("unknown dependency: node {node} depends on unknown node {dep}")]
    UnknownDependency { node: String, dep: String },
    #[error("cycle detected in workflow dependencies")]
    Cycle,
    #[error("node {0} has no type defined (need prompt, bash, or interactive)")]
    NoType(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowConfig {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub nodes: Vec<NodeDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodeDef {
    pub id: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// AI prompt node
    pub prompt: Option<String>,
    /// Model override for AI node
    pub model: Option<String>,
    /// Inject HNN mesh context into prompt
    #[serde(default)]
    pub context: bool,
    /// Bash command node
    pub bash: Option<String>,
    /// Expected exit code for bash (default 0)
    #[serde(default = "default_exit_code")]
    pub exit_code: i32,
    /// Timeout in seconds for bash
    pub timeout: Option<u64>,
    /// Interactive approval gate
    #[serde(default)]
    pub interactive: bool,
    /// Parallel sub-nodes (list of node definitions)
    #[serde(default)]
    pub parallel: Vec<NodeDef>,
    /// Loop configuration
    pub r#loop: Option<LoopConfig>,
    /// Agent node fields (CrewAI-inspired)
    pub role: Option<String>,
    pub goal: Option<String>,
    pub backstory: Option<String>,
    pub task: Option<String>,
}

fn default_exit_code() -> i32 {
    0
}

impl WorkflowConfig {
    /// Load a workflow config from a YAML file.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_str(&content)
    }

    /// Parse a workflow config from a YAML string.
    pub fn from_str(yaml: &str) -> Result<Self, ConfigError> {
        let config: WorkflowConfig = serde_yaml::from_str(yaml)?;
        config.validate()?;
        Ok(config)
    }

    /// Validate the config: check for duplicates, unknown deps, cycles.
    fn validate(&self) -> Result<(), ConfigError> {
        let mut ids: HashMap<&str, usize> = HashMap::new();
        for node in &self.nodes {
            *ids.entry(&node.id).or_insert(0) += 1;
            if ids.get(node.id.as_str()).copied().unwrap_or(0) > 1 {
                return Err(ConfigError::DuplicateNodeId(node.id.clone()));
            }
        }
        for node in &self.nodes {
            for dep in &node.depends_on {
                if !ids.contains_key(dep.as_str()) {
                    return Err(ConfigError::UnknownDependency {
                        node: node.id.clone(),
                        dep: dep.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Convert node definitions into typed NodeTypes.
    pub fn build_nodes(&self) -> Result<Vec<crate::node::Node>, ConfigError> {
        self.nodes
            .iter()
            .map(|def| {
                let node_type = if let Some(prompt) = &def.prompt {
                    NodeType::Ai(AiNode {
                        prompt: prompt.clone(),
                        model: def.model.clone(),
                        context: def.context,
                    })
                } else if def.role.is_some() && def.task.is_some() {
                    // CrewAI-inspired Agent node
                    NodeType::Agent(AgentNode {
                        role: def.role.clone().unwrap_or_default(),
                        goal: def.goal.clone().unwrap_or_default(),
                        backstory: def.backstory.clone().unwrap_or_default(),
                        task: def.task.clone().unwrap_or_default(),
                        context: def.context,
                        model: def.model.clone(),
                    })
                } else if let Some(cmd) = &def.bash {
                    NodeType::Bash(BashNode {
                        command: cmd.clone(),
                        expected_exit_code: def.exit_code,
                        timeout: def.timeout,
                    })
                } else if def.interactive {
                    NodeType::Approval(ApprovalNode {
                        prompt: def
                            .r#loop
                            .as_ref()
                            .map(|l| l.prompt.clone())
                            .unwrap_or_default(),
                    })
                } else if !def.parallel.is_empty() {
                    let sub_nodes: Result<Vec<_>, ConfigError> = def
                        .parallel
                        .iter()
                        .map(|sub| {
                            Ok(crate::node::Node {
                                id: sub.id.clone(),
                                depends_on: sub.depends_on.clone(),
                                node_type: NodeType::Ai(AiNode {
                                    prompt: sub.prompt.clone().unwrap_or_default(),
                                    model: sub.model.clone(),
                                    context: sub.context,
                                }),
                                loop_config: sub.r#loop.clone(),
                            })
                        })
                        .collect();
                    NodeType::Parallel(ParallelNode { nodes: sub_nodes? })
                } else {
                    return Err(ConfigError::NoType(def.id.clone()));
                };

                Ok(crate::node::Node {
                    id: def.id.clone(),
                    depends_on: def.depends_on.clone(),
                    node_type,
                    loop_config: def.r#loop.clone(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal() {
        let yaml = r#"
name: test
description: "minimal test"
nodes:
  - id: step1
    prompt: "do something"
"#;
        let config = WorkflowConfig::from_str(yaml).unwrap();
        assert_eq!(config.name, "test");
        assert_eq!(config.nodes.len(), 1);
    }

    #[test]
    fn reject_duplicate_ids() {
        let yaml = r#"
name: dup
nodes:
  - id: x
    prompt: "a"
  - id: x
    prompt: "b"
"#;
        assert!(WorkflowConfig::from_str(yaml).is_err());
    }

    #[test]
    fn reject_unknown_dep() {
        let yaml = r#"
name: bad
nodes:
  - id: a
    prompt: "a"
  - id: b
    depends_on: [a, missing]
    prompt: "b"
"#;
        assert!(WorkflowConfig::from_str(yaml).is_err());
    }
}
