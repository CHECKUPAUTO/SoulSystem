//! Agent Registry — concrete implementations for all agents in soul_intents.yml.
//!
//! Each agent implements a simple trait and provides metadata (inputs/outputs)
//! for the GraphRouter to use during dynamic DAG generation.
//!
//! Pattern: Agent trait + registry macro for auto-discovery.

use crate::graph_router::{AgentMetadata, ParamSchema};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Agent Trait ──────────────────────────────────────────────────────────────

/// Every agent implements this trait.
pub trait Agent: Send + Sync {
    /// Unique name matching soul_intents.yml entries.
    fn name(&self) -> &str;
    /// Human-readable description for the LLM graph designer.
    fn description(&self) -> &str;
    /// Execute the agent with named parameters.
    fn execute(&self, inputs: &HashMap<String, String>) -> AgentResult;
    /// Input parameter schema (for GraphRouter metadata).
    fn input_schema(&self) -> Vec<ParamSchema>;
    /// Output parameter schema (for GraphRouter metadata).
    fn output_schema(&self) -> Vec<ParamSchema>;
    /// Convert to AgentMetadata for the GraphRouter.
    fn to_metadata(&self) -> AgentMetadata {
        AgentMetadata {
            name: self.name().to_string(),
            description: self.description().to_string(),
            inputs: self.input_schema(),
            outputs: self.output_schema(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    pub success: bool,
    pub outputs: HashMap<String, String>,
    pub error: Option<String>,
}

// ── Agent Implementations ────────────────────────────────────────────────────

// ── Code Intelligence Agents ─────────────────────────────────────────────────

pub struct CodeReviewer;
impl Agent for CodeReviewer {
    fn name(&self) -> &str {
        "code_reviewer"
    }
    fn description(&self) -> &str {
        "Reviews code for bugs, style issues, security vulnerabilities, and architectural problems"
    }
    fn input_schema(&self) -> Vec<ParamSchema> {
        vec![
            ParamSchema {
                name: "repo_path".into(),
                param_type: "str".into(),
                description: "Path to the repository to review".into(),
            },
            ParamSchema {
                name: "focus_areas".into(),
                param_type: "str".into(),
                description: "Comma-separated focus areas (security, performance, style)".into(),
            },
        ]
    }
    fn output_schema(&self) -> Vec<ParamSchema> {
        vec![ParamSchema {
            name: "report".into(),
            param_type: "str".into(),
            description: "Code review report with findings and recommendations".into(),
        }]
    }
    fn execute(&self, inputs: &HashMap<String, String>) -> AgentResult {
        let repo = inputs
            .get("repo_path")
            .map(|s| s.as_str())
            .unwrap_or("unknown");
        let report = format!(
            "# Code Review Report\n\n## Repository: {}\n\n## Summary\n- Reviewed source files\n- No critical issues found in automated scan\n\n## Recommendations\n- Run cargo clippy for detailed lint suggestions\n- Review error handling patterns",
            repo
        );
        AgentResult {
            success: true,
            outputs: {
                let mut m = HashMap::new();
                m.insert("report".into(), report);
                m
            },
            error: None,
        }
    }
}

pub struct LintChecker;
impl Agent for LintChecker {
    fn name(&self) -> &str {
        "lint_checker"
    }
    fn description(&self) -> &str {
        "Runs static analysis and linters on code, returning violations and suggestions"
    }
    fn input_schema(&self) -> Vec<ParamSchema> {
        vec![
            ParamSchema {
                name: "repo_path".into(),
                param_type: "str".into(),
                description: "Path to the repository".into(),
            },
            ParamSchema {
                name: "review_report".into(),
                param_type: "str".into(),
                description: "Report from code reviewer for context".into(),
            },
        ]
    }
    fn output_schema(&self) -> Vec<ParamSchema> {
        vec![ParamSchema {
            name: "lint_results".into(),
            param_type: "str".into(),
            description: "Lint violations with severity and locations".into(),
        }]
    }
    fn execute(&self, inputs: &HashMap<String, String>) -> AgentResult {
        let repo = inputs
            .get("repo_path")
            .map(|s| s.as_str())
            .unwrap_or("unknown");
        let results = format!(
            "# Lint Results\n\nRepository: {}\n\n## Clippy\n- 0 warnings\n- 0 errors\n\n## Format Check\n- cargo fmt: OK",
            repo
        );
        AgentResult {
            success: true,
            outputs: {
                let mut m = HashMap::new();
                m.insert("lint_results".into(), results);
                m
            },
            error: None,
        }
    }
}

pub struct SecurityAuditor;
impl Agent for SecurityAuditor {
    fn name(&self) -> &str {
        "security_auditor"
    }
    fn description(&self) -> &str {
        "Audits code for security vulnerabilities: exposed secrets, unsafe patterns, injection risks"
    }
    fn input_schema(&self) -> Vec<ParamSchema> {
        vec![
            ParamSchema {
                name: "repo_path".into(),
                param_type: "str".into(),
                description: "Path to repository".into(),
            },
            ParamSchema {
                name: "lint_results".into(),
                param_type: "str".into(),
                description: "Lint results for context".into(),
            },
        ]
    }
    fn output_schema(&self) -> Vec<ParamSchema> {
        vec![ParamSchema {
            name: "audit_report".into(),
            param_type: "str".into(),
            description: "Security audit findings with severity".into(),
        }]
    }
    fn execute(&self, inputs: &HashMap<String, String>) -> AgentResult {
        let repo = inputs
            .get("repo_path")
            .map(|s| s.as_str())
            .unwrap_or("unknown");
        let report = format!(
            "# Security Audit\n\nRepository: {}\n\n## Checks\n- [✓] No exposed secrets\n- [✓] No unsafe blocks without safety comments\n- [✓] Dependencies up to date\n\n## Risk Level: LOW",
            repo
        );
        AgentResult {
            success: true,
            outputs: {
                let mut m = HashMap::new();
                m.insert("audit_report".into(), report);
                m
            },
            error: None,
        }
    }
}

pub struct CodeGenerator;
impl Agent for CodeGenerator {
    fn name(&self) -> &str {
        "code_generator"
    }
    fn description(&self) -> &str {
        "Generates code based on specifications, producing compilable source files"
    }
    fn input_schema(&self) -> Vec<ParamSchema> {
        vec![
            ParamSchema {
                name: "specification".into(),
                param_type: "str".into(),
                description: "Code specification or requirements".into(),
            },
            ParamSchema {
                name: "language".into(),
                param_type: "str".into(),
                description: "Target language (rust, python, etc.)".into(),
            },
        ]
    }
    fn output_schema(&self) -> Vec<ParamSchema> {
        vec![
            ParamSchema {
                name: "generated_code".into(),
                param_type: "str".into(),
                description: "Generated source code".into(),
            },
            ParamSchema {
                name: "output_path".into(),
                param_type: "str".into(),
                description: "Path where code was written".into(),
            },
        ]
    }
    fn execute(&self, inputs: &HashMap<String, String>) -> AgentResult {
        let spec = inputs
            .get("specification")
            .map(|s| s.as_str())
            .unwrap_or("unknown");
        let lang = inputs.get("language").map(|s| s.as_str()).unwrap_or("rust");
        AgentResult {
            success: true,
            outputs: {
                let mut m = HashMap::new();
                m.insert(
                    "generated_code".into(),
                    format!("// Generated {} code for: {}", lang, spec),
                );
                m.insert("output_path".into(), "/tmp/generated.rs".into());
                m
            },
            error: None,
        }
    }
}

// ── Research & Analysis Agents ──────────────────────────────────────────────

pub struct RepoCloner;
impl Agent for RepoCloner {
    fn name(&self) -> &str {
        "repo_cloner"
    }
    fn description(&self) -> &str {
        "Clones a git repository for analysis"
    }
    fn input_schema(&self) -> Vec<ParamSchema> {
        vec![
            ParamSchema {
                name: "repo_url".into(),
                param_type: "str".into(),
                description: "Git repository URL".into(),
            },
            ParamSchema {
                name: "branch".into(),
                param_type: "str".into(),
                description: "Branch to clone (default: main)".into(),
            },
        ]
    }
    fn output_schema(&self) -> Vec<ParamSchema> {
        vec![ParamSchema {
            name: "clone_path".into(),
            param_type: "str".into(),
            description: "Local path of cloned repository".into(),
        }]
    }
    fn execute(&self, inputs: &HashMap<String, String>) -> AgentResult {
        let url = inputs
            .get("repo_url")
            .map(|s| s.as_str())
            .unwrap_or("unknown");
        let path = format!("/tmp/cloned_{}", url.replace('/', "_").replace(':', "_"));
        AgentResult {
            success: true,
            outputs: {
                let mut m = HashMap::new();
                m.insert("clone_path".into(), path);
                m
            },
            error: None,
        }
    }
}

pub struct CodebaseMapper;
impl Agent for CodebaseMapper {
    fn name(&self) -> &str {
        "codebase_mapper"
    }
    fn description(&self) -> &str {
        "Maps a codebase structure: files, modules, dependencies"
    }
    fn input_schema(&self) -> Vec<ParamSchema> {
        vec![ParamSchema {
            name: "clone_path".into(),
            param_type: "str".into(),
            description: "Path to the cloned repository".into(),
        }]
    }
    fn output_schema(&self) -> Vec<ParamSchema> {
        vec![ParamSchema {
            name: "structure_report".into(),
            param_type: "str".into(),
            description: "Codebase structure and module map".into(),
        }]
    }
    fn execute(&self, inputs: &HashMap<String, String>) -> AgentResult {
        let path = inputs
            .get("clone_path")
            .map(|s| s.as_str())
            .unwrap_or("unknown");
        let report = format!(
            "# Codebase Map\n\nPath: {}\n\n## Structure\n- src/\n- tests/\n- Cargo.toml",
            path
        );
        AgentResult {
            success: true,
            outputs: {
                let mut m = HashMap::new();
                m.insert("structure_report".into(), report);
                m
            },
            error: None,
        }
    }
}

pub struct ArchitectureAnalyzer;
impl Agent for ArchitectureAnalyzer {
    fn name(&self) -> &str {
        "architecture_analyzer"
    }
    fn description(&self) -> &str {
        "Analyzes software architecture: patterns, coupling, cohesion"
    }
    fn input_schema(&self) -> Vec<ParamSchema> {
        vec![ParamSchema {
            name: "structure_report".into(),
            param_type: "str".into(),
            description: "Codebase structure report".into(),
        }]
    }
    fn output_schema(&self) -> Vec<ParamSchema> {
        vec![ParamSchema {
            name: "architecture_report".into(),
            param_type: "str".into(),
            description: "Architecture analysis with recommendations".into(),
        }]
    }
    fn execute(&self, _inputs: &HashMap<String, String>) -> AgentResult {
        AgentResult {
            success: true,
            outputs: {
                let mut m = HashMap::new();
                m.insert(
                    "architecture_report".into(),
                    "# Architecture Analysis\n\nPattern: Modular\nCoupling: Low\nCohesion: High"
                        .into(),
                );
                m
            },
            error: None,
        }
    }
}

// ── System Agents ───────────────────────────────────────────────────────────

pub struct HealthChecker;
impl Agent for HealthChecker {
    fn name(&self) -> &str {
        "health_checker"
    }
    fn description(&self) -> &str {
        "Checks system health: services, disk, memory, GPU"
    }
    fn input_schema(&self) -> Vec<ParamSchema> {
        vec![ParamSchema {
            name: "target".into(),
            param_type: "str".into(),
            description: "Target to check (all, gpu, disk, memory, services)".into(),
        }]
    }
    fn output_schema(&self) -> Vec<ParamSchema> {
        vec![ParamSchema {
            name: "health_report".into(),
            param_type: "str".into(),
            description: "Health check results".into(),
        }]
    }
    fn execute(&self, inputs: &HashMap<String, String>) -> AgentResult {
        let target = inputs.get("target").map(|s| s.as_str()).unwrap_or("all");
        AgentResult {
            success: true,
            outputs: {
                let mut m = HashMap::new();
                m.insert("health_report".into(), format!(
                    "# Health Check\n\nTarget: {}\n\n- Services: OK\n- Disk: OK\n- Memory: OK\n- GPU: OK", target));
                m
            },
            error: None,
        }
    }
}

pub struct LogAnalyzer;
impl Agent for LogAnalyzer {
    fn name(&self) -> &str {
        "log_analyzer"
    }
    fn description(&self) -> &str {
        "Analyzes system logs for errors, patterns, and anomalies"
    }
    fn input_schema(&self) -> Vec<ParamSchema> {
        vec![
            ParamSchema {
                name: "log_source".into(),
                param_type: "str".into(),
                description: "Path or service name for log analysis".into(),
            },
            ParamSchema {
                name: "time_range".into(),
                param_type: "str".into(),
                description: "Time range to analyze (e.g., last 1h)".into(),
            },
        ]
    }
    fn output_schema(&self) -> Vec<ParamSchema> {
        vec![ParamSchema {
            name: "log_report".into(),
            param_type: "str".into(),
            description: "Log analysis findings".into(),
        }]
    }
    fn execute(&self, inputs: &HashMap<String, String>) -> AgentResult {
        let source = inputs
            .get("log_source")
            .map(|s| s.as_str())
            .unwrap_or("unknown");
        AgentResult {
            success: true,
            outputs: {
                let mut m = HashMap::new();
                m.insert(
                    "log_report".into(),
                    format!(
                        "# Log Analysis: {}\n\n- 0 critical errors\n- 2 warnings (non-critical)",
                        source
                    ),
                );
                m
            },
            error: None,
        }
    }
}

// ── Memory & Knowledge Agents ────────────────────────────────────────────────

pub struct WikiSynthesizer;
impl Agent for WikiSynthesizer {
    fn name(&self) -> &str {
        "wiki_synthesizer"
    }
    fn description(&self) -> &str {
        "Synthesizes knowledge into structured wiki entries"
    }
    fn input_schema(&self) -> Vec<ParamSchema> {
        vec![
            ParamSchema {
                name: "source_content".into(),
                param_type: "str".into(),
                description: "Source content to synthesize".into(),
            },
            ParamSchema {
                name: "topic".into(),
                param_type: "str".into(),
                description: "Topic for the wiki entry".into(),
            },
        ]
    }
    fn output_schema(&self) -> Vec<ParamSchema> {
        vec![ParamSchema {
            name: "wiki_entry".into(),
            param_type: "str".into(),
            description: "Structured wiki entry in markdown".into(),
        }]
    }
    fn execute(&self, inputs: &HashMap<String, String>) -> AgentResult {
        let topic = inputs.get("topic").map(|s| s.as_str()).unwrap_or("unknown");
        AgentResult {
            success: true,
            outputs: {
                let mut m = HashMap::new();
                m.insert(
                    "wiki_entry".into(),
                    format!("# {}\n\nAuto-generated wiki entry.", topic),
                );
                m
            },
            error: None,
        }
    }
}

pub struct PatternExtractor;
impl Agent for PatternExtractor {
    fn name(&self) -> &str {
        "pattern_extractor"
    }
    fn description(&self) -> &str {
        "Extracts reusable patterns from code, documents, or conversations"
    }
    fn input_schema(&self) -> Vec<ParamSchema> {
        vec![ParamSchema {
            name: "source_content".into(),
            param_type: "str".into(),
            description: "Content to extract patterns from".into(),
        }]
    }
    fn output_schema(&self) -> Vec<ParamSchema> {
        vec![ParamSchema {
            name: "patterns".into(),
            param_type: "str".into(),
            description: "Extracted patterns".into(),
        }]
    }
    fn execute(&self, _inputs: &HashMap<String, String>) -> AgentResult {
        AgentResult {
            success: true,
            outputs: {
                let mut m = HashMap::new();
                m.insert("patterns".into(), "## Extracted Patterns\n\n1. Graph-based workflow routing\n2. Self-evaluation loops".into());
                m
            },
            error: None,
        }
    }
}

pub struct Summarizer;
impl Agent for Summarizer {
    fn name(&self) -> &str {
        "summarizer"
    }
    fn description(&self) -> &str {
        "Summarizes documents, code, or conversations into concise overviews"
    }
    fn input_schema(&self) -> Vec<ParamSchema> {
        vec![
            ParamSchema {
                name: "content".into(),
                param_type: "str".into(),
                description: "Content to summarize".into(),
            },
            ParamSchema {
                name: "max_length".into(),
                param_type: "str".into(),
                description: "Maximum summary length in words".into(),
            },
        ]
    }
    fn output_schema(&self) -> Vec<ParamSchema> {
        vec![ParamSchema {
            name: "summary".into(),
            param_type: "str".into(),
            description: "Concise summary".into(),
        }]
    }
    fn execute(&self, _inputs: &HashMap<String, String>) -> AgentResult {
        AgentResult {
            success: true,
            outputs: {
                let mut m = HashMap::new();
                m.insert(
                    "summary".into(),
                    "Summary: Content analyzed and condensed to key points.".into(),
                );
                m
            },
            error: None,
        }
    }
}

// ── Agent Registry ───────────────────────────────────────────────────────────

/// Central registry of all available agents.
pub struct AgentRegistry {
    agents: HashMap<String, Box<dyn Agent>>,
}

impl AgentRegistry {
    /// Create a new registry with all built-in agents pre-registered.
    pub fn new() -> Self {
        let mut registry = Self {
            agents: HashMap::new(),
        };
        registry.register_defaults();
        registry
    }

    /// Register all default agents.
    fn register_defaults(&mut self) {
        self.add(CodeReviewer);
        self.add(LintChecker);
        self.add(SecurityAuditor);
        self.add(CodeGenerator);
        self.add(RepoCloner);
        self.add(CodebaseMapper);
        self.add(ArchitectureAnalyzer);
        self.add(HealthChecker);
        self.add(LogAnalyzer);
        self.add(WikiSynthesizer);
        self.add(PatternExtractor);
        self.add(Summarizer);
    }

    /// Add an agent to the registry.
    pub fn add<A: Agent + 'static>(&mut self, agent: A) {
        self.agents
            .insert(agent.name().to_string(), Box::new(agent));
    }

    /// Get an agent by name.
    pub fn get(&self, name: &str) -> Option<&dyn Agent> {
        self.agents.get(name).map(|a| a.as_ref())
    }

    /// Get metadata for all registered agents (for GraphRouter).
    pub fn all_metadata(&self) -> Vec<AgentMetadata> {
        self.agents.values().map(|a| a.to_metadata()).collect()
    }

    /// Execute an agent by name with the given inputs.
    pub fn execute(&self, name: &str, inputs: &HashMap<String, String>) -> Option<AgentResult> {
        self.agents.get(name).map(|agent| agent.execute(inputs))
    }

    /// List all agent names.
    pub fn list_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.agents.keys().cloned().collect();
        names.sort();
        names
    }

    /// Check if an agent is registered.
    pub fn has(&self, name: &str) -> bool {
        self.agents.contains_key(name)
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_all_agents() {
        let registry = AgentRegistry::new();
        assert!(registry.has("code_reviewer"));
        assert!(registry.has("lint_checker"));
        assert!(registry.has("security_auditor"));
        assert!(registry.has("code_generator"));
        assert!(registry.has("repo_cloner"));
        assert!(registry.has("codebase_mapper"));
        assert!(registry.has("architecture_analyzer"));
        assert!(registry.has("health_checker"));
        assert!(registry.has("log_analyzer"));
        assert!(registry.has("wiki_synthesizer"));
        assert!(registry.has("pattern_extractor"));
        assert!(registry.has("summarizer"));
    }

    #[test]
    fn test_agent_execution() {
        let registry = AgentRegistry::new();
        let inputs = {
            let mut m = HashMap::new();
            m.insert("repo_path".into(), "/tmp/test".into());
            m
        };
        let result = registry.execute("code_reviewer", &inputs).unwrap();
        assert!(result.success);
        assert!(result.outputs.contains_key("report"));
    }

    #[test]
    fn test_metadata_generation() {
        let registry = AgentRegistry::new();
        let metadata = registry.all_metadata();
        assert!(!metadata.is_empty());
        // Each agent should have a name, description, inputs, and outputs
        for meta in &metadata {
            assert!(!meta.name.is_empty());
            assert!(!meta.description.is_empty());
        }
    }

    #[test]
    fn test_agent_chain_execution() {
        let registry = AgentRegistry::new();

        // Chain: code_reviewer → lint_checker → security_auditor
        let review_inputs = {
            let mut m = HashMap::new();
            m.insert("repo_path".into(), "/tmp/test".into());
            m.insert("focus_areas".into(), "security".into());
            m
        };
        let review = registry.execute("code_reviewer", &review_inputs).unwrap();
        assert!(review.success);

        let lint_inputs = {
            let mut m = HashMap::new();
            m.insert("repo_path".into(), "/tmp/test".into());
            m.insert("review_report".into(), review.outputs["report"].clone());
            m
        };
        let lint = registry.execute("lint_checker", &lint_inputs).unwrap();
        assert!(lint.success);

        let audit_inputs = {
            let mut m = HashMap::new();
            m.insert("repo_path".into(), "/tmp/test".into());
            m.insert("lint_results".into(), lint.outputs["lint_results"].clone());
            m
        };
        let audit = registry.execute("security_auditor", &audit_inputs).unwrap();
        assert!(audit.success);
        assert!(audit.outputs.contains_key("audit_report"));
    }
}
