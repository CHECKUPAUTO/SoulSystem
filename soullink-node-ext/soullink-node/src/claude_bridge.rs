//! Claude Code Bridge — bidirectional integration between SoulLink and Claude Code.
//!
//! ## Three integration layers (mix of audit options 1+3+4):
//!
//! ### Level 1 — API Endpoint (`POST /api/claude/mutate`)
//! Claude Code proposes mutations via HTTP. SoulLink applies them atomically,
//! measures outcomes, and reports back. Supports rollback on harmful mutations.
//!
//! ### Level 3 — Evolution Accelerator
//! When stagnation is detected (no fitness improvement for N generations),
//! SoulLink invokes Claude Code to analyze the genome and propose targeted mutations.
//!
//! ### Level 4 — Bridge Daemon
//! Background task that continuously:
//! - Observes brain state (fitness, entropy, stagnation)
//! - Detects stagnation (flat fitness for >500 ticks)
//! - Invokes Claude Code (`claude -p "...soullink analysis..."`) with context
//! - Applies Claude Code's proposals via the API
//!

//! SoulLink detects stagnation → invokes claude → Claude Code analyzes →
//! proposes mutation JSON → SoulLink applies → measures outcome → reports back


use serde::{Deserialize, Serialize};
use tracing::{info, warn, error};

// ═══════════════════════════════════════════════════════════════
// DATA TYPES
// ═══════════════════════════════════════════════════════════════

/// A mutation proposal from Claude Code.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClaudeMutationProposal {
    /// Human-readable description of what this mutation does
    pub description: String,
    /// The mutation kind (matches crate::evolution::Mutation variant names)
    pub kind: String,
    /// JSON-encoded parameters for the mutation
    pub params: serde_json::Value,
    /// Claude Code's confidence in this proposal (0.0–1.0)
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    /// Priority: "high", "medium", "low"
    #[serde(default = "default_priority")]
    pub priority: String,
    /// Optional reasoning trace
    #[serde(default)]
    pub reasoning: String,
}

fn default_confidence() -> f64 { 0.7 }
fn default_priority() -> String { "medium".into() }

/// Result of applying a Claude-proposed mutation.
#[derive(Debug, Clone, Serialize)]
pub struct ClaudeMutationResult {
    /// Whether the mutation was applied
    pub applied: bool,
    /// Fitness before mutation
    pub fitness_before: f64,
    /// Fitness after mutation (0.0 if rolled back)
    pub fitness_after: f64,
    /// Was the mutation rolled back (too harmful)?
    pub rolled_back: bool,
    /// Outcome classification
    pub outcome: String,
    /// Ticks elapsed since application
    pub ticks_elapsed: u64,
    /// Human-readable report
    pub report: String,
}

/// Request body for `POST /api/claude/mutate`.
#[derive(Debug, Clone, Deserialize)]
pub struct ClaudeMutateRequest {
    pub proposal: ClaudeMutationProposal,
    /// If true, apply immediately. If false, just validate.
    #[serde(default = "default_true")]
    pub apply: bool,
    /// If true, rollback if fitness drops > rollback_threshold
    #[serde(default = "default_true")]
    pub atomic: bool,
    /// Maximum fitness drop before rollback
    #[serde(default = "default_rollback_threshold")]
    pub rollback_threshold: f64,
}

fn default_true() -> bool { true }
fn default_rollback_threshold() -> f64 { 0.05 }

/// Brain context sent to Claude Code for analysis.
#[derive(Debug, Clone, Serialize)]
pub struct SoulLinkContext {
    pub tick: u64,
    pub generation: u64,
    pub fitness: f64,
    pub peak_fitness: f64,
    pub stagnation_generations: u64,
    pub energy_avg: f64,
    pub arousal: f64,
    pub stress_mode: String,
    pub module_count: usize,
    pub neuron_count: usize,
    pub synapse_count: usize,
    pub cortex_enabled: bool,
    pub mutation_count: u64,
    pub recent_outcomes: Vec<String>,
    pub top_params: serde_json::Value,
    pub meta_status: serde_json::Value,
}

/// Status of the Claude bridge itself.
#[derive(Debug, Clone, Serialize)]
pub struct BridgeStatus {
    pub enabled: bool,
    pub last_invocation_tick: u64,
    pub last_proposal: Option<String>,
    pub proposals_applied: u64,
    pub proposals_succeeded: u64,
    pub proposals_rolled_back: u64,
    pub stagnation_ticks: u64,
    pub claude_available: bool,
    pub daemon_running: bool,
}

// ═══════════════════════════════════════════════════════════════
// CLAUDE BRIDGE STATE
// ═══════════════════════════════════════════════════════════════

/// Manages the Claude Code integration state.
#[derive(Debug)]
pub struct ClaudeBridge {
    pub enabled: bool,
    /// Cooldown between Claude invocations (ticks)
    pub invoke_cooldown: u64,
    /// How many ticks of flat fitness before invoking Claude
    pub stagnation_threshold: u64,
    /// Path to the claude binary
    pub claude_path: String,
    /// Maximum proposals per session
    pub max_proposals: u64,

    // State
    pub last_invocation_tick: u64,
    pub last_proposal: Option<ClaudeMutationProposal>,
    pub proposals_applied: u64,
    pub proposals_succeeded: u64,
    pub proposals_rolled_back: u64,
    pub stagnation_ticks: u64,
    pub claude_available: bool,
    pub daemon_running: bool,

    // Pending proposal (set by daemon or API, consumed by simulation)
    pub pending_proposal: Option<ClaudeMutationProposal>,

    // Snapshot for rollback (fitness before applying)
    pub pre_mutation_snapshot: Option<f64>,
}

impl ClaudeBridge {
    pub fn new() -> Self {
        // Check if claude binary is available
        let claude_available = std::process::Command::new("claude")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        let claude_path = if claude_available {
            "claude".to_string()
        } else {
            // Try common paths
            for path in &["/usr/local/bin/claude", "/usr/bin/claude", "/home/fuomag9/.local/bin/claude"] {
                if std::path::Path::new(path).exists() {
                    return Self {
                        enabled: true,
                        invoke_cooldown: 500,
                        stagnation_threshold: 300,
                        claude_path: path.to_string(),
                        max_proposals: 20,
                        last_invocation_tick: 0,
                        last_proposal: None,
                        proposals_applied: 0,
                        proposals_succeeded: 0,
                        proposals_rolled_back: 0,
                        stagnation_ticks: 0,
                        claude_available: true,
                        daemon_running: false,
                        pending_proposal: None,
                        pre_mutation_snapshot: None,
                    };
                }
            }
            "claude".to_string()
        };

        Self {
            enabled: claude_available,
            invoke_cooldown: 500,
            stagnation_threshold: 300,
            claude_path,
            max_proposals: 20,
            last_invocation_tick: 0,
            last_proposal: None,
            proposals_applied: 0,
            proposals_succeeded: 0,
            proposals_rolled_back: 0,
            stagnation_ticks: 0,
            claude_available,
            daemon_running: false,
            pending_proposal: None,
            pre_mutation_snapshot: None,
        }
    }

    /// Check if Claude is ready to be invoked (cooldown elapsed, under max proposals).
    pub fn can_invoke(&self, current_tick: u64) -> bool {
        self.enabled
            && self.claude_available
            && self.proposals_applied < self.max_proposals
            && current_tick.saturating_sub(self.last_invocation_tick) >= self.invoke_cooldown
    }

    /// Build a prompt for Claude Code with SoulLink context.
    pub fn build_claude_prompt(&self, ctx: &SoulLinkContext) -> String {
        format!(
            r#"You are the SoulLink auto-evolution accelerator. Analyze this brain state and propose ONE targeted mutation to improve fitness.

## Brain State
- Tick: {tick}, Generation: {gen}
- Current fitness: {fitness:.4}, Peak ever: {peak:.4}, Stagnation: {stag} gens
- Energy avg: {energy:.3}, Arousal: {arousal:.3}, Stress: {stress}
- Modules: {mods}, Neurons: {neurons}, Synapses: {syns}
- Cortex: {cortex}
- Total mutations applied: {mutations}
- Recent outcomes: {outcomes}

## Your Task
1. Analyze why the brain is {stuck}
2. Propose ONE mutation that targets the ROOT CAUSE
3. Output ONLY valid JSON in this exact format:

```json
{{
  "kind": "<MutationVariantName>",
  "description": "<brief human-readable description>",
  "params": {{ <mutation-specific parameters> }},
  "confidence": <0.0-1.0>,
  "priority": "high|medium|low",
  "reasoning": "<why this mutation should help>"
}}
```

Available mutation kinds and their params:
- AddNeuron: {{ "module": <idx>, "count": <1-100> }}
- PruneNeurons: {{ "module": <idx>, "fraction": <0.01-0.5> }}
- AddModule: {{ "name": "<name>", "count": <50-2000> }}
- MutateParam: {{ "param": "<tau|homeostatic_rate|stdp_learning_rate|prune_threshold>", "delta": <-0.1-0.1> }}
- MutateSynapticDensity: {{ "source": <idx>, "target": <idx>, "density": <0.0-1.0> }}
- MutateCortexConfig: {{ "d_model": <4-128>, "state_dim": <2-32>, "n_layers": <1-8> }}
- MutateActivationScript: {{ "module": <idx>, "script": "<rhai code>" }}
- MutatePlasticityScript: {{ "script": "<rhai code>" }}
- MutateModuleMetabolicRate: {{ "module": <idx>, "delta": <-0.1-0.1> }}
- MutateModulePriority: {{ "module": <idx>, "delta": <-0.1-0.1> }}
- MergeModules: {{ "a": <idx>, "b": <idx>, "new_name": "<name>" }}
- SplitModule: {{ "module": <idx>, "new_name": "<name>" }}

Rules:
- Module indices are 0-based. Valid range: 0..{mods} (there are {mods} modules)
- DO NOT suggest adding modules if module count >= 20
- Focus on the weakest fitness dimension
- Confidence must reflect your actual certainty (not just 0.9 blindly)
- PREFER EXPLOITATION over pure exploration
- Respond with ONLY the JSON, no other text
"#,
            tick = ctx.tick,
            gen = ctx.generation,
            fitness = ctx.fitness,
            peak = ctx.peak_fitness,
            stag = ctx.stagnation_generations,
            energy = ctx.energy_avg,
            arousal = ctx.arousal,
            stress = ctx.stress_mode,
            mods = ctx.module_count,
            neurons = ctx.neuron_count,
            syns = ctx.synapse_count,
            cortex = if ctx.cortex_enabled { "active" } else { "disabled" },
            mutations = ctx.mutation_count,
            outcomes = ctx.recent_outcomes.join(", "),
            stuck = if ctx.stagnation_generations > 100 { "stagnating badly" } else if ctx.stagnation_generations > 50 { "plateauing" } else { "stable" },
        )
    }

    /// Invoke Claude Code synchronously with a prompt.
    /// Returns the parsed mutation proposal, or None on failure.
    pub fn invoke_claude(&self, prompt: &str) -> Option<ClaudeMutationProposal> {
        info!("Invoking Claude Code for mutation proposal...");

        match std::process::Command::new(&self.claude_path)
            .arg("-p")
            .arg(prompt)
            .arg("--output-format")
            .arg("text")
            .arg("--no-ansi")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
        {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    warn!("Claude Code invocation failed (exit {:?}): {}",
                          output.status.code(), stderr.lines().last().unwrap_or(""));
                    return None;
                }

                let stdout = String::from_utf8_lossy(&output.stdout);
                // Try to extract JSON from Claude's response
                let json_str = Self::extract_json(&stdout);

                match serde_json::from_str::<ClaudeMutationProposal>(&json_str) {
                    Ok(proposal) => {
                        info!("Claude Code proposed: {} (confidence={:.2})",
                              proposal.description, proposal.confidence);
                        Some(proposal)
                    }
                    Err(e) => {
                        warn!("Failed to parse Claude's proposal: {}. Raw output (first 500 chars): {:.500}", e, stdout);
                        None
                    }
                }
            }
            Err(e) => {
                error!("Failed to spawn claude process: {}", e);
                None
            }
        }
    }

    /// Extract JSON from Claude's response (handles ```json fences, etc.).
    fn extract_json(raw: &str) -> String {
        // If it has ```json ... ``` fences, extract content inside
        if let Some(start) = raw.find("```json") {
            let after_start = &raw[start + 7..];
            if let Some(end) = after_start.find("```") {
                return after_start[..end].trim().to_string();
            }
        }
        // If it has ``` ... ``` (without json tag)
        if let Some(start) = raw.find("```") {
            let after_start = &raw[start + 3..];
            if let Some(end) = after_start.find("```") {
                let inner = after_start[..end].trim();
                // Check if the first line is a language tag (skip it)
                if let Some(newline) = inner.find('\n') {
                    let after_lang = inner[newline..].trim();
                    if after_lang.starts_with('{') {
                        return after_lang.to_string();
                    }
                }
            }
        }

        // Try to find a standalone JSON object
        if let Some(start) = raw.find('{') {
            if let Some(end) = raw.rfind('}') {
                let candidate = &raw[start..=end];
                if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                    return candidate.to_string();
                }
            }
        }

        // Return raw trimmed
        raw.trim().to_string()
    }

    /// Try to detect stagnation from evolution data and invoke Claude if needed.
    /// Returns true if Claude wasinvoked.
    pub fn try_invoke_on_stagnation(
        &mut self,
        current_tick: u64,
        ctx: &SoulLinkContext,
    ) -> bool {
        if !self.can_invoke(current_tick) {
            return false;
        }

        let should_invoke = ctx.stagnation_generations >= self.stagnation_threshold;

        if !should_invoke {
            self.stagnation_ticks = 0;
            return false;
        }

        self.stagnation_ticks = ctx.stagnation_generations;
        info!(
            "Stagnation detected ({} gens, fitness={:.4}, peak={:.4}) — invoking Claude Code",
            ctx.stagnation_generations, ctx.fitness, ctx.peak_fitness
        );

        let prompt = self.build_claude_prompt(ctx);
        match self.invoke_claude(&prompt) {
            Some(proposal) => {
                self.last_invocation_tick = current_tick;
                self.last_proposal = Some(proposal.clone());
                self.pending_proposal = Some(proposal);
                true
            }
            None => {
                warn!("Claude Code invocation produced no valid proposal");
                self.last_invocation_tick = current_tick;
                false
            }
        }
    }

    /// Record the outcome of applying a Claude-proposed mutation.
    pub fn record_outcome(&mut self, fitness_before: f64, fitness_after: f64, rolled_back: bool) {
        self.proposals_applied += 1;
        if rolled_back {
            self.proposals_rolled_back += 1;
            info!("Claude mutation rolled back (fitness: {:.4} → {:.4})", fitness_before, fitness_after);
        } else if fitness_after > fitness_before {
            self.proposals_succeeded += 1;
            info!("Claude mutation succeeded (fitness: {:.4} → {:.4})", fitness_before, fitness_after);
        } else {
            info!("Claude mutation had no positive effect (fitness: {:.4} → {:.4})", fitness_before, fitness_after);
        }
    }

    /// Success rate of Claude proposals.
    pub fn success_rate(&self) -> f64 {
        if self.proposals_applied == 0 {
            return 0.0;
        }
        self.proposals_succeeded as f64 / self.proposals_applied as f64
    }

    /// API status report.
    pub fn api_status(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": self.enabled,
            "claude_available": self.claude_available,
            "claude_path": self.claude_path,
            "daemon_running": self.daemon_running,
            "last_invocation_tick": self.last_invocation_tick,
            "invoke_cooldown": self.invoke_cooldown,
            "stagnation_threshold": self.stagnation_threshold,
            "proposals_applied": self.proposals_applied,
            "proposals_succeeded": self.proposals_succeeded,
            "proposals_rolled_back": self.proposals_rolled_back,
            "max_proposals": self.max_proposals,
            "success_rate": self.success_rate(),
            "stagnation_ticks": self.stagnation_ticks,
            "pending_proposal": self.pending_proposal.as_ref().map(|p| p.description.clone()),
        })
    }
}

impl Default for ClaudeBridge {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════

/// Build a SoulLinkContext from the current brain state.
pub fn build_context(
    tick: u64,
    fitness: f64,
    peak_fitness: f64,
    generation: u64,
    stagnation_generations: u64,
    energy_avg: f64,
    arousal: f64,
    stress_mode: &str,
    module_count: usize,
    neuron_count: usize,
    synapse_count: usize,
    cortex_enabled: bool,
    mutation_count: u64,
    meta_status: serde_json::Value,
) -> SoulLinkContext {
    SoulLinkContext {
        tick,
        generation,
        fitness,
        peak_fitness,
        stagnation_generations,
        energy_avg,
        arousal,
        stress_mode: stress_mode.to_string(),
        module_count,
        neuron_count,
        synapse_count,
        cortex_enabled,
        mutation_count,
        recent_outcomes: Vec::new(),
        top_params: serde_json::json!({}),
        meta_status,
    }
}

/// Apply a ClaudeMutationProposal (converts to a Mutation, applies via evolution engine).
pub fn apply_claude_proposal(
    proposal: &ClaudeMutationProposal,
    brain: &mut crate::Brain,
) -> Result<ClaudeMutationResult, String> {
    let fitness_before = brain.evolution.fitness.current.overall;

    // Parse the mutation from proposal params
    let mutation = parse_claude_mutation(proposal)?;

    // Apply using take pattern to avoid borrow conflicts
    let mut e = std::mem::take(&mut brain.evolution);
    e.apply_mutation(brain, &mutation);
    e.fitness.sample(brain);
    let fitness_after = e.fitness.current.overall;

    // Classify outcome
    let outcome = if fitness_after > fitness_before + 0.005 {
        e.track_mutation_result(&mutation, fitness_before, fitness_after);
        "improved"
    } else if fitness_after < fitness_before - 0.02 {
        "worsened"
    } else {
        "no_effect"
    };

    e.generation += 1;
    e.mutation_count += 1;
    brain.evolution = e;

    Ok(ClaudeMutationResult {
        applied: true,
        fitness_before,
        fitness_after,
        rolled_back: false,
        outcome: outcome.to_string(),
        ticks_elapsed: 0,
        report: format!(
            "Mutation '{}' applied: fitness {:.4} → {:.4} ({})",
            proposal.description, fitness_before, fitness_after, outcome
        ),
    })
}

/// Convert a ClaudeMutationProposal to an evolution Mutation.
pub fn parse_claude_mutation(proposal: &ClaudeMutationProposal) -> Result<crate::evolution::Mutation, String> {
    let p = &proposal.params;
    let kind = proposal.kind.as_str();

    match kind {
        "AddNeuron" => {
            let module = p.get("module").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let count = p.get("count").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
            Ok(crate::evolution::Mutation::AddNeuron { module, count: count.max(1) })
        }
        "PruneNeurons" => {
            let module = p.get("module").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let fraction = p.get("fraction").and_then(|v| v.as_f64()).unwrap_or(0.1);
            Ok(crate::evolution::Mutation::PruneNeurons { module, fraction: fraction.clamp(0.0, 1.0) })
        }
        "AddModule" => {
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("claude_mod").to_string();
            let count = p.get("count").and_then(|v| v.as_u64()).unwrap_or(200) as usize;
            Ok(crate::evolution::Mutation::AddModule {
                name,
                count: count.max(50).min(2000),
                x: 0.0, y: 0.0,
                color: "#cc88ff".into(),
            })
        }
        "MergeModules" => {
            let a = p.get("a").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let b = p.get("b").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
            let new_name = p.get("new_name").and_then(|v| v.as_str()).unwrap_or("merged").to_string();
            Ok(crate::evolution::Mutation::MergeModules { a, b, new_name })
        }
        "SplitModule" => {
            let module = p.get("module").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let new_name = p.get("new_name").and_then(|v| v.as_str()).unwrap_or("split").to_string();
            Ok(crate::evolution::Mutation::SplitModule { module, new_name })
        }
        "MutateParam" => {
            let param = p.get("param").and_then(|v| v.as_str()).unwrap_or("tau").to_string();
            let delta = p.get("delta").and_then(|v| v.as_f64()).unwrap_or(0.0);
            Ok(crate::evolution::Mutation::MutateParam { param, delta: delta.clamp(-0.1, 0.1) })
        }
        "MutateSynapticDensity" => {
            let source = p.get("source").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let target = p.get("target").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
            let density = p.get("density").and_then(|v| v.as_f64()).unwrap_or(0.3) as f32;
            Ok(crate::evolution::Mutation::MutateSynapticDensity { source, target, density })
        }
        "MutateCortexConfig" => {
            let d_model = p.get("d_model").and_then(|v| v.as_u64()).unwrap_or(16) as usize;
            let state_dim = p.get("state_dim").and_then(|v| v.as_u64()).unwrap_or(4) as usize;
            let n_layers = p.get("n_layers").and_then(|v| v.as_u64()).unwrap_or(2) as usize;
            Ok(crate::evolution::Mutation::MutateCortexConfig {
                d_model: d_model.max(4).min(128),
                state_dim: state_dim.max(2).min(32),
                n_layers: n_layers.max(1).min(8),
            })
        }
        "MutateActivationScript" => {
            let module = p.get("module").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let script = p.get("script").and_then(|v| v.as_str()).unwrap_or("sigmoid(potential)").to_string();
            Ok(crate::evolution::Mutation::MutateActivationScript { module, script })
        }
        "MutatePlasticityScript" => {
            let script = p.get("script").and_then(|v| v.as_str()).unwrap_or("clamp(weight + pre * post * 0.01, 0, 1)").to_string();
            Ok(crate::evolution::Mutation::MutatePlasticityScript { script })
        }
        "MutateModuleMetabolicRate" => {
            let module = p.get("module").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let delta = p.get("delta").and_then(|v| v.as_f64()).unwrap_or(0.0);
            Ok(crate::evolution::Mutation::MutateModuleMetabolicRate { module, delta })
        }
        "MutateModulePriority" => {
            let module = p.get("module").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let delta = p.get("delta").and_then(|v| v.as_f64()).unwrap_or(0.0);
            Ok(crate::evolution::Mutation::MutateModulePriority { module, delta })
        }
        _ => Err(format!("Unknown mutation kind: {}. Available: AddNeuron, PruneNeurons, AddModule, MergeModules, SplitModule, MutateParam, MutateSynapticDensity, MutateCortexConfig, MutateActivationScript, MutatePlasticityScript, MutateModuleMetabolicRate, MutateModulePriority", kind)),
    }
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_fenced() {
        let raw = r#"Here's my analysis:
```json
{"kind":"AddNeuron","description":"add neurons","params":{"module":0,"count":5},"confidence":0.8,"priority":"high","reasoning":"test"}
```
That's all."#;

        let extracted = ClaudeBridge::extract_json(raw);
        let proposal: ClaudeMutationProposal = serde_json::from_str(&extracted).unwrap();
        assert_eq!(proposal.kind, "AddNeuron");
        assert_eq!(proposal.confidence, 0.8);
    }

    #[test]
    fn test_extract_json_standalone() {
        let raw = r#"{"kind":"MutateParam","description":"tune tau","params":{"param":"tau","delta":0.02},"confidence":0.6,"priority":"medium","reasoning":"faster decay"}"#;

        let extracted = ClaudeBridge::extract_json(raw);
        let proposal: ClaudeMutationProposal = serde_json::from_str(&extracted).unwrap();
        assert_eq!(proposal.kind, "MutateParam");
        assert_eq!(proposal.params["param"], "tau");
    }

    #[test]
    fn test_parse_claude_mutation_add_neuron() {
        let proposal = ClaudeMutationProposal {
            description: "add 5 neurons".into(),
            kind: "AddNeuron".into(),
            params: serde_json::json!({"module": 2, "count": 5}),
            confidence: 0.9,
            priority: "high".into(),
            reasoning: "test".into(),
        };

        let mutation = parse_claude_mutation(&proposal).unwrap();
        assert!(matches!(mutation, crate::evolution::Mutation::AddNeuron { module: 2, count: 5 }));
    }

    #[test]
    fn test_parse_claude_mutation_mutate_param() {
        let proposal = ClaudeMutationProposal {
            description: "tune tau".into(),
            kind: "MutateParam".into(),
            params: serde_json::json!({"param": "tau", "delta": -0.03}),
            confidence: 0.7,
            priority: "medium".into(),
            reasoning: "".into(),
        };

        let mutation = parse_claude_mutation(&proposal).unwrap();
        assert!(matches!(mutation, crate::evolution::Mutation::MutateParam { param, delta } if param == "tau" && delta == -0.03));
    }

    #[test]
    fn test_parse_claude_mutation_mutate_cortex() {
        let proposal = ClaudeMutationProposal {
            description: "resize cortex".into(),
            kind: "MutateCortexConfig".into(),
            params: serde_json::json!({"d_model": 32, "state_dim": 8, "n_layers": 3}),
            confidence: 0.8,
            priority: "high".into(),
            reasoning: "".into(),
        };

        let mutation = parse_claude_mutation(&proposal).unwrap();
        assert!(matches!(mutation, crate::evolution::Mutation::MutateCortexConfig { d_model: 32, state_dim: 8, n_layers: 3 }));
    }

    #[test]
    fn test_parse_claude_mutation_unknown_kind() {
        let proposal = ClaudeMutationProposal {
            description: "bad".into(),
            kind: "InvalidKind".into(),
            params: serde_json::json!({}),
            confidence: 0.5,
            priority: "low".into(),
            reasoning: "".into(),
        };

        assert!(parse_claude_mutation(&proposal).is_err());
    }

    #[test]
    fn test_bridge_new_initializes_fields() {
        let bridge = ClaudeBridge::new();
        assert!(bridge.proposals_applied == 0);
        assert!(bridge.success_rate() == 0.0);
        assert!(bridge.api_status().is_object());
    }

    #[test]
    fn test_can_invoke_cooldown() {
        let mut bridge = ClaudeBridge::new();
        bridge.enabled = true;
        bridge.claude_available = true;
        bridge.last_invocation_tick = 0;
        bridge.invoke_cooldown = 100;

        assert!(bridge.can_invoke(150)); // cooldown elapsed
        bridge.last_invocation_tick = 100;
        assert!(!bridge.can_invoke(150)); // cooldown not elapsed (50 < 100)
    }

    #[test]
    fn test_can_invoke_disabled() {
        let bridge = ClaudeBridge::new();
        let mut b = bridge;
        b.enabled = false;
        assert!(!b.can_invoke(500));
    }

    #[test]
    fn test_record_outcome_success() {
        let mut bridge = ClaudeBridge::new();
        bridge.record_outcome(0.5, 0.55, false);
        assert_eq!(bridge.proposals_applied, 1);
        assert_eq!(bridge.proposals_succeeded, 1);
        assert_eq!(bridge.proposals_rolled_back, 0);
    }

    #[test]
    fn test_record_outcome_rollback() {
        let mut bridge = ClaudeBridge::new();
        bridge.record_outcome(0.5, 0.3, true);
        assert_eq!(bridge.proposals_applied, 1);
        assert_eq!(bridge.proposals_succeeded, 0);
        assert_eq!(bridge.proposals_rolled_back, 1);
    }

    #[test]
    fn test_success_rate_calculation() {
        let mut bridge = ClaudeBridge::new();
        bridge.record_outcome(0.5, 0.6, false); // success
        bridge.record_outcome(0.5, 0.4, true);  // rollback
        bridge.record_outcome(0.5, 0.5, false); // no effect (not success)
        assert!((bridge.success_rate() - 1.0/3.0).abs() < 0.01);
    }

    #[test]
    fn test_build_context_has_all_fields() {
        let ctx = build_context(
            1000, 0.75, 0.9, 42, 5, 0.8, 0.3,
            "calm", 8, 5000, 12000, true, 150,
            serde_json::json!({"state": "ok"}),
        );

        assert_eq!(ctx.tick, 1000);
        assert_eq!(ctx.fitness, 0.75);
        assert_eq!(ctx.generation, 42);
        assert_eq!(ctx.module_count, 8);
        assert_eq!(ctx.neuron_count, 5000);
    }

    #[test]
    fn test_build_claude_prompt_contains_context() {
        let ctx = SoulLinkContext {
            tick: 1000,
            generation: 42,
            fitness: 0.65,
            peak_fitness: 0.9,
            stagnation_generations: 200,
            energy_avg: 0.7,
            arousal: 0.4,
            stress_mode: "calm".into(),
            module_count: 8,
            neuron_count: 5000,
            synapse_count: 12000,
            cortex_enabled: true,
            mutation_count: 150,
            recent_outcomes: vec!["improved".into(), "no_effect".into()],
            top_params: serde_json::json!({}),
            meta_status: serde_json::json!({"state": "ok"}),
        };

        let bridge = ClaudeBridge::new();
        let prompt = bridge.build_claude_prompt(&ctx);

        assert!(prompt.contains("0.65"));
        assert!(prompt.contains("200"));
        assert!(prompt.contains("calm"));
        assert!(prompt.contains("AddNeuron"));
        assert!(prompt.contains("MutateParam"));
    }
}
