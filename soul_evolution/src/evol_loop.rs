use crate::analyzer::Analyzer;
use crate::audit::Auditor;
use crate::generator::Generator;
use crate::meta_evolution::MetaEvolver;
use crate::types::*;
use crate::validator::Validator;

const MAX_META_INFERENCE_LENGTH: usize = 2000;

pub struct SelfImprovementLoop {
    pub config: EvolutionConfig,
    pub iteration: u64,
    pub history: Vec<IterationRecord>,
    pub health_trajectory: Vec<f64>,
    meta_strategies: Vec<MetaStrategy>,
    pub meta_evolver: MetaEvolver,
}

struct MetaStrategy {
    name: String,
    #[allow(dead_code)]
    description: String,
    used_count: u64,
    success_count: u64,
    effectiveness: f64,
}

impl SelfImprovementLoop {
    pub fn new(config: EvolutionConfig) -> Self {
        Self {
            config,
            iteration: 0,
            history: Vec::new(),
            health_trajectory: Vec::new(),
            meta_evolver: MetaEvolver::new(),
            meta_strategies: vec![
                MetaStrategy {
                    name: "generate-missing-language-agents".to_string(),
                    description: "Detect uncovered languages and generate reviewer agents".to_string(),
                    used_count: 0,
                    success_count: 0,
                    effectiveness: 0.0,
                },
                MetaStrategy {
                    name: "improve-low-quality-agents".to_string(),
                    description: "Rewrite agents below quality threshold".to_string(),
                    used_count: 0,
                    success_count: 0,
                    effectiveness: 0.0,
                },
                MetaStrategy {
                    name: "fix-incomplete-agents".to_string(),
                    description: "Patch agents missing critical fields".to_string(),
                    used_count: 0,
                    success_count: 0,
                    effectiveness: 0.0,
                },
                MetaStrategy {
                    name: "rebalance-models".to_string(),
                    description: "Adjust model assignments for cost/performance".to_string(),
                    used_count: 0,
                    success_count: 0,
                    effectiveness: 0.0,
                },
            ],
        }
    }

    /// Exécute une itération complète de la boucle d'auto-amélioration récursive.
    ///
    /// The loop follows the Anthropic recursive self-improvement paradigm:
    /// 1. AUDIT  → Measure current state of the agent ecosystem
    /// 2. ANALYZE → Find gaps, bottlenecks (Amdahl's law)
    /// 3. GENERATE → Create improvement proposals
    /// 4. VALIDATE → Verify proposals are well-formed
    /// 5. APPLY  → Deploy improvements (optional, controlled by auto_apply)
    /// 6. LEARN  → Update meta-strategies based on results
    /// 7. REPEAT → Next iteration audits the improved ecosystem
    pub fn run_iteration(&mut self) -> Result<IterationRecord, String> {
        let start = std::time::Instant::now();

        tracing::info!(
            iteration = self.iteration + 1,
            "=== Recursive Self-Improvement Loop: Iteration {} ===",
            self.iteration + 1
        );

        // PHASE 1: AUDIT
        tracing::info!("Phase 1/6: Auditing agent ecosystem...");
        let audit_start = std::time::Instant::now();
        let agent_audits = Auditor::audit_agents(&self.config);
        let file_counts = Auditor::count_files(&self.config);
        let report = Auditor::generate_report(&self.config, &agent_audits, &file_counts);
        let audit_duration = audit_start.elapsed().as_millis();
        tracing::info!(
            agents = report.agent_count,
            health = format!("{:.1}%", report.ecosystem_health * 100.0),
            duration_ms = audit_duration,
            "Audit complete"
        );

        // PHASE 2: ANALYZE
        tracing::info!("Phase 2/6: Analyzing gaps and bottlenecks...");
        let analysis_start = std::time::Instant::now();
        let analysis = Analyzer::analyze(&report);
        let analysis_duration = analysis_start.elapsed().as_millis();
        tracing::info!(
            gaps = analysis.gaps.len(),
            bottlenecks = analysis.bottlenecks.len(),
            duration_ms = analysis_duration,
            "Analysis complete"
        );

        // PHASE 3: GENERATE
        tracing::info!("Phase 3/6: Generating improvement proposals...");
        let generate_start = std::time::Instant::now();
        let proposals = Generator::propose_improvements(&report, &analysis, &self.config);
        let generate_duration = generate_start.elapsed().as_millis();
        tracing::info!(
            proposals = proposals.len(),
            duration_ms = generate_duration,
            "Generation complete"
        );

        // PHASE 4: VALIDATE
        tracing::info!("Phase 4/6: Validating proposals...");
        let validate_start = std::time::Instant::now();
        let validation_results = Validator::validate_batch(&proposals);
        let validation_duration = validate_start.elapsed().as_millis();

        let valid_proposals: Vec<&ImprovementProposal> = proposals
            .iter()
            .enumerate()
            .filter(|(i, _)| validation_results.get(*i).map_or(false, |r| r.1.valid))
            .map(|(_, p)| p)
            .collect();

        tracing::info!(
            valid = valid_proposals.len(),
            invalid = proposals.len() - valid_proposals.len(),
            duration_ms = validation_duration,
            "Validation complete"
        );

        // PHASE 5: APPLY
        let mut improvements = Vec::new();
        if !valid_proposals.is_empty() {
            tracing::info!("Phase 5/6: Applying improvements...");
            let apply_start = std::time::Instant::now();

            for proposal in &valid_proposals {
                let result = self.apply_single(proposal);
                if result.success {
                    self.record_strategy_success(&proposal.gap, true);
                } else {
                    self.record_strategy_success(&proposal.gap, false);
                }
                improvements.push(result);
            }

            let apply_duration = apply_start.elapsed().as_millis();
            tracing::info!(
                applied = improvements.iter().filter(|r| r.success).count(),
                failed = improvements.iter().filter(|r| !r.success).count(),
                duration_ms = apply_duration,
                "Apply complete"
            );
        } else {
            tracing::info!("Phase 5/6: No improvements to apply");
        }

        // PHASE 6: LEARN (basic meta-learning)
        tracing::info!("Phase 6/14: Meta-learning...");
        let meta_start = std::time::Instant::now();
        let meta_inference = self.meta_learn(&report, &analysis, &improvements);
        let _meta_duration = meta_start.elapsed().as_millis();

        // PHASES 7-14: 7-PILLAR META-EVOLUTION (Godel, Darwin, STOP, Self-Play, Reflexion, OPRO, Explosion)
        tracing::info!("Phases 7-14: Running meta-evolution cycle (7 pillars)...");
        let meta_cycle_result = self.meta_evolver.cycle(&report, &agent_audits, &self.config);
        if self.config.verbose {
            println!("{}", meta_cycle_result.format());
        }

        let prev_health = self.health_trajectory.last().copied().unwrap_or(0.0);
        let health_delta = report.ecosystem_health - prev_health;
        self.health_trajectory.push(report.ecosystem_health);

        let total_duration = start.elapsed().as_millis();
        self.iteration += 1;

        let record = IterationRecord {
            iteration: self.iteration,
            timestamp: chrono::Utc::now(),
            audit: report,
            analysis,
            improvements,
            health_delta,
            duration_ms: total_duration as u64,
            meta_inference,
        };

        self.history.push(record.clone());

        // Persist the latest iteration record for cross-session status
        let _ = Self::persist_record(&record, &self.config.output_dir);

        if let Some(max_iter) = self.config.max_iterations {
            if self.iteration < max_iter {
                tracing::info!(
                    "Scheduling next iteration ({} of {})",
                    self.iteration + 1,
                    max_iter
                );
                // Recursive call: the loop calls itself
                // This is the key insight from the article: compounding acceleration
                // Each iteration audits the IMPROVED ecosystem, creating a positive feedback loop
                return self.run_iteration();
            }
        }

        tracing::info!(
            iteration = self.iteration,
            duration_ms = total_duration,
            health = format!("{:.1}%", record.audit.ecosystem_health * 100.0),
            delta = format!("{:+.2}%", health_delta * 100.0),
            "=== Loop iteration complete ==="
        );

        Ok(record)
    }

    fn apply_single(&self, proposal: &ImprovementProposal) -> ImprovementResult {
        let path = match &proposal.target_path {
            Some(p) => p,
            None => {
                return ImprovementResult {
                    proposal_id: proposal.id.clone(),
                    success: false,
                    path: None,
                    message: "No target path specified".to_string(),
                    warnings: vec![],
                };
            }
        };

        if !self.config.auto_apply {
            return ImprovementResult {
                proposal_id: proposal.id.clone(),
                success: false,
                path: Some(path.clone()),
                message: "Dry-run mode (auto_apply is false). Use --apply to write.".to_string(),
                warnings: vec![],
            };
        }

        if path.exists() {
            let backup_path = path.with_extension("md.bak");
            if let Err(e) = std::fs::copy(path, &backup_path) {
                return ImprovementResult {
                    proposal_id: proposal.id.clone(),
                    success: false,
                    path: Some(path.clone()),
                    message: format!("Cannot create backup: {}", e),
                    warnings: vec![],
                };
            }
        }

        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return ImprovementResult {
                    proposal_id: proposal.id.clone(),
                    success: false,
                    path: Some(path.clone()),
                    message: format!("Cannot create directory: {}", e),
                    warnings: vec![],
                };
            }
        }

        match std::fs::write(path, &proposal.template_content) {
            Ok(_) => ImprovementResult {
                proposal_id: proposal.id.clone(),
                success: true,
                path: Some(path.clone()),
                message: format!("Written to {}", path.display()),
                warnings: vec![],
            },
            Err(e) => ImprovementResult {
                proposal_id: proposal.id.clone(),
                success: false,
                path: Some(path.clone()),
                message: format!("Write failed: {}", e),
                warnings: vec![],
            },
        }
    }

    fn record_strategy_success(&mut self, category: &GapCategory, success: bool) {
        let strategy_name = match category {
            GapCategory::MissingLanguageAgent => "generate-missing-language-agents",
            GapCategory::LowQualityAgent => "improve-low-quality-agents",
            GapCategory::IncompleteAgent => "fix-incomplete-agents",
            GapCategory::UnderrepresentedModel => "rebalance-models",
            _ => return,
        };

        for strategy in &mut self.meta_strategies {
            if strategy.name == strategy_name {
                strategy.used_count += 1;
                if success {
                    strategy.success_count += 1;
                }
                strategy.effectiveness = if strategy.used_count > 0 {
                    strategy.success_count as f64 / strategy.used_count as f64
                } else {
                    0.0
                };
                break;
            }
        }
    }

    fn meta_learn(
        &self,
        report: &AuditReport,
        _analysis: &AnalysisResult,
        improvements: &[ImprovementResult],
    ) -> String {
        let mut insights = Vec::new();

        let health = report.ecosystem_health;
        if health < 0.3 {
            insights.push(format!(
                "CRITICAL: Ecosystem health is {:.0}%. Immediate intervention required.",
                health * 100.0
            ));
        } else if health < 0.6 {
            insights.push(format!(
                "WARNING: Ecosystem health is {:.0}%. Systematic improvement needed.",
                health * 100.0
            ));
        } else if health >= 0.8 {
            insights.push(format!(
                "HEALTHY: Ecosystem at {:.0}%. Focus on edge cases and advanced agents.",
                health * 100.0
            ));
        }

        let success_count = improvements.iter().filter(|r| r.success).count();
        let total_count = improvements.len();
        if total_count > 0 {
            let rate = success_count as f64 / total_count as f64;
            if rate < 0.5 {
                insights.push(format!(
                    "Low improvement success rate ({:.0}%): reviewing generation strategy.",
                    rate * 100.0
                ));
            } else {
                insights.push(format!(
                    "Good improvement rate ({:.0}%). Strategies are effective.",
                    rate * 100.0
                ));
            }
        }

        let effective_strategies: Vec<&MetaStrategy> = self
            .meta_strategies
            .iter()
            .filter(|s| s.used_count > 0)
            .collect();

        if !effective_strategies.is_empty() {
            let mut strategies_report = String::from("Strategy effectiveness: ");
            for s in &effective_strategies {
                let eff = format!("{:.0}%", s.effectiveness * 100.0);
                strategies_report.push_str(&format!("{}={} (used {}x), ", s.name, eff, s.used_count));
            }
            insights.push(strategies_report);
        }

        let iterations = self.iteration;
        if iterations > 0 && self.health_trajectory.len() >= 2 {
            let first = self.health_trajectory.first().copied().unwrap_or(0.0);
            let current = self.health_trajectory.last().copied().unwrap_or(0.0);
            let total_delta = current - first;
            if total_delta > 0.05 {
                insights.push(format!(
                    "Ecosystem improved {:.1}% over {} iteration(s). Compounding acceleration detected.",
                    total_delta * 100.0,
                    iterations
                ));
            } else if total_delta < -0.05 {
                insights.push(format!(
                    "Ecosystem declined {:.1}% over {} iteration(s). Strategy review needed.",
                    total_delta.abs() * 100.0,
                    iterations
                ));
            }
        }

        let health_trend = self.compute_health_trend();
        if let Some(trend) = health_trend {
            insights.push(format!("Health trend: {:+.4} per iteration", trend));
        }

        let mut inf = insights.join("\n  • ");
        if !insights.is_empty() {
            inf = "  • ".to_string() + &inf;
        }

        if inf.len() > MAX_META_INFERENCE_LENGTH {
            inf.truncate(MAX_META_INFERENCE_LENGTH);
            inf.push_str("...");
        }

        inf
    }

    pub fn compute_health_trend(&self) -> Option<f64> {
        let n = self.health_trajectory.len();
        if n < 2 {
            return None;
        }

        let indices: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let values: Vec<f64> = self.health_trajectory.clone();

        let mean_x: f64 = indices.iter().sum::<f64>() / n as f64;
        let mean_y: f64 = values.iter().sum::<f64>() / n as f64;

        let mut num = 0.0;
        let mut den = 0.0;
        for i in 0..n {
            let dx = indices[i] - mean_x;
            let dy = values[i] - mean_y;
            num += dx * dy;
            den += dx * dx;
        }

        if den.abs() < 1e-10 {
            None
        } else {
            Some(num / den)
        }
    }

    pub fn get_summary(&self) -> String {
        let mut out = String::new();
        use std::fmt::Write;

        let _ = writeln!(
            out,
            "╔══════════════════════════════════════════════╗"
        );
        let _ = writeln!(
            out,
            "║     RECURSIVE SELF-IMPROVEMENT LOOP          ║"
        );
        let _ = writeln!(
            out,
            "╚══════════════════════════════════════════════╝"
        );
        let _ = writeln!(out);
        let _ = writeln!(out, "  Iterations:         {}", self.iteration);
        let _ = writeln!(
            out,
            "  Current health:     {:.1}%",
            self.health_trajectory
                .last()
                .copied()
                .unwrap_or(0.0)
                * 100.0
        );
        if let Some(trend) = self.compute_health_trend() {
            let direction = if trend > 0.0 { "↑ improving" } else { "↓ declining" };
            let _ = writeln!(out, "  Trend:              {} ({:+.4}/iter)", direction, trend);
        }
        let _ = writeln!(
            out,
            "  History records:    {}",
            self.history.len()
        );
        let _ = writeln!(out);

        if !self.meta_strategies.is_empty() {
            let _ = writeln!(out, "── Meta-Strategy Effectiveness ──");
            for s in &self.meta_strategies {
                if s.used_count > 0 {
                    let bar_len = (s.effectiveness * 20.0) as usize;
                    let bar = "█".repeat(bar_len);
                    let _ = writeln!(
                        out,
                        "  {:<35} │ {} {:>5.0}% ({}x)",
                        s.name,
                        bar,
                        s.effectiveness * 100.0,
                        s.used_count
                    );
                } else {
                    let _ = writeln!(
                        out,
                        "  {:<35} │ (unused)",
                        s.name
                    );
                }
            }
        }

        let _ = writeln!(
            out,
            "────────────────────────────────────────────"
        );
        out
    }

    pub fn last_record(&self) -> Option<&IterationRecord> {
        self.history.last()
    }

    /// Save the latest iteration record to disk for cross-session persistence
    fn persist_record(record: &IterationRecord, output_dir: &std::path::Path) -> Result<(), String> {
        let _ = std::fs::create_dir_all(output_dir);
        let path = output_dir.join("evolution-state.txt");

        let content = format!(
            "iteration={}\n\
             timestamp={}\n\
             health={:.4}\n\
             health_delta={:+.4}\n\
             agent_count={}\n\
             gap_count={}\n\
             bottleneck_count={}\n\
             improvement_count={}\n\
             improvements_applied={}\n\
             duration_ms={}\n\
             meta_inference={}\n",
            record.iteration,
            record.timestamp.format("%Y-%m-%dT%H:%M:%SZ"),
            record.audit.ecosystem_health,
            record.health_delta,
            record.audit.agent_count,
            record.analysis.gaps.len(),
            record.analysis.bottlenecks.len(),
            record.improvements.len(),
            record.improvements.iter().filter(|r| r.success).count(),
            record.duration_ms,
            record.meta_inference.replace('\n', " | "),
        );

        std::fs::write(&path, &content)
            .map_err(|e| format!("Failed to write evolution state: {}", e))
    }

    /// Load the last persisted iteration record (cross-session)
    pub fn load_last_state(output_dir: &std::path::Path) -> Option<Self> {
        let path = output_dir.join("evolution-state.txt");
        if !path.exists() {
            return None;
        }
        let content = std::fs::read_to_string(&path).ok()?;

        let mut health_trajectory = Vec::new();
        let mut iteration: u64 = 0;
        for line in content.lines() {
            if let Some((k, v)) = line.split_once('=') {
                match k {
                    "health" => {
                        if let Ok(h) = v.parse::<f64>() {
                            health_trajectory.push(h);
                        }
                    }
                    "iteration" => {
                        iteration = v.parse().unwrap_or(0);
                    }
                    _ => {}
                }
            }
        }

        let mut config = EvolutionConfig::default();
        config.output_dir = output_dir.to_path_buf();

        let mut loop_state = Self::new(config);
        loop_state.iteration = iteration;
        loop_state.health_trajectory = health_trajectory;
        Some(loop_state)
    }
}

/// Point d'entrée de la boucle d'évolution autonome.
///
/// This is the main function that mirrors the `extern "C" fn(*mut u8)` pattern
/// from soul_agent_runtime, enabling this loop to be called as a cognitive step
/// by the scheduler (closing the meta-loop: the scheduler runs agents that evolve agents).
pub fn run_evolution_cycle(
    agents_dir: Option<&str>,
    auto_apply: bool,
    max_iterations: Option<u64>,
    verbose: bool,
) -> Result<IterationRecord, String> {
    let mut config = EvolutionConfig::default();
    if let Some(dir) = agents_dir {
        config.agents_dir = std::path::PathBuf::from(dir);
    }
    config.auto_apply = auto_apply;
    config.max_iterations = max_iterations;
    config.verbose = verbose;

    let mut loop_engine = SelfImprovementLoop::new(config);

    if verbose {
        tracing::info!("Starting self-improvement loop...");
        tracing::info!(
            "Config: auto_apply={}, max_iterations={:?}",
            auto_apply,
            max_iterations
        );
    }

    let record = loop_engine.run_iteration()?;

    if verbose {
        tracing::info!(
            "Loop complete: iteration {}, health {:.1}%, {} improvements, took {}ms",
            record.iteration,
            record.audit.ecosystem_health * 100.0,
            record.improvements.len(),
            record.duration_ms
        );
    }

    Ok(record)
}
