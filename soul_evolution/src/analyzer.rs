use crate::types::*;
use std::collections::HashSet;

pub struct Analyzer;

impl Analyzer {
    pub fn analyze(report: &AuditReport) -> AnalysisResult {
        let mut gaps = Vec::new();
        let mut bottlenecks = Vec::new();
        let mut recommendations = Vec::new();

        let agent_names: HashSet<String> = report
            .audits
            .iter()
            .map(|a| a.agent.name.to_lowercase())
            .collect();

        let agent_descriptions: Vec<&str> = report
            .audits
            .iter()
            .map(|a| a.agent.description.as_str())
            .collect();

        let covered_languages: HashSet<String> = report.language_coverage.keys().cloned().collect();

        Self::find_missing_language_agents(
            &agent_names,
            &agent_descriptions,
            &covered_languages,
            &mut gaps,
        );

        Self::find_low_quality_agents(report, &mut gaps);

        Self::find_model_imbalance(report, &mut gaps);

        Self::find_tool_coverage_gaps(report, &mut gaps);

        Self::find_coverage_duplicates(report, &mut gaps);

        Self::identify_bottlenecks(report, &mut bottlenecks, &mut recommendations);

        Self::generate_recommendations(&gaps, &bottlenecks, &mut recommendations);

        gaps.sort_by(|a, b| {
            b.severity
                .score()
                .partial_cmp(&a.severity.score())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let quality_histogram = Self::build_quality_histogram(report);

        AnalysisResult {
            gaps,
            bottlenecks,
            recommendations,
            quality_histogram,
        }
    }

    fn is_language_covered(
        lang: &str,
        agent_names: &HashSet<String>,
        descriptions: &[&str],
    ) -> bool {
        let lang_lower = lang.to_lowercase();
        if agent_names.iter().any(|name| name.contains(&lang_lower)) {
            return true;
        }
        if descriptions
            .iter()
            .any(|desc| desc.to_lowercase().contains(&lang_lower))
        {
            return true;
        }
        false
    }

    fn find_missing_language_agents(
        agent_names: &HashSet<String>,
        descriptions: &[&str],
        covered_languages: &HashSet<String>,
        gaps: &mut Vec<Gap>,
    ) {
        let popular_missing = &[
            "php", "ruby", "scala", "elixir", "clojure", "haskell", "lua", "r", "julia", "zig",
        ];

        for &lang in popular_missing {
            if covered_languages.contains(lang) {
                continue;
            }
            if Self::is_language_covered(lang, agent_names, descriptions) {
                continue;
            }

            let reviewer_name = format!("{}-reviewer", lang);
            let resolver_name = format!("{}-build-resolver", lang);

            gaps.push(Gap {
                category: GapCategory::MissingLanguageAgent,
                severity: Severity::Medium,
                title: format!("Missing {} reviewer agent", lang),
                description: format!(
                    "No dedicated {} review agent found. Consider adding '{}' and possibly '{}'.",
                    lang, reviewer_name, resolver_name
                ),
                suggestion: format!(
                    "Create agents/{}.md with a review-focused prompt for {} code.",
                    reviewer_name, lang
                ),
            });
        }

        let all_languages: Vec<&str> = KNOWN_LANGUAGES.to_vec();
        for &lang in &all_languages {
            if Self::is_language_covered(lang, agent_names, descriptions) {
                continue;
            }

            let reviewer_name = format!("{}-reviewer", lang);
            let has_reviewer = agent_names.contains(&reviewer_name.to_lowercase());
            let has_resolver = agent_names.contains(&format!("{}-build-resolver", lang));

            if !has_reviewer && !has_resolver {
                gaps.push(Gap {
                    category: GapCategory::MissingLanguageAgent,
                    severity: Severity::Low,
                    title: format!("No agent for {}", lang),
                    description: format!("Language '{}' has no dedicated agent.", lang),
                    suggestion: format!("Consider adding a {}-reviewer agent.", lang),
                });
            } else if !has_reviewer && has_resolver {
                gaps.push(Gap {
                    category: GapCategory::MissingLanguageAgent,
                    severity: Severity::Low,
                    title: format!("Missing {} reviewer (has resolver)", lang),
                    description: format!(
                        "{} has a build resolver but no dedicated reviewer.",
                        lang
                    ),
                    suggestion: format!("Create {}-reviewer to complement the resolver.", lang),
                });
            }
        }
    }

    fn find_low_quality_agents(report: &AuditReport, gaps: &mut Vec<Gap>) {
        for audit in &report.audits {
            if audit.quality_score < 0.4 {
                gaps.push(Gap {
                    category: GapCategory::LowQualityAgent,
                    severity: Severity::High,
                    title: format!("Low quality agent: {}", audit.agent.name),
                    description: format!(
                        "Agent '{}' has a quality score of {:.0}%. Issues: {}",
                        audit.agent.name,
                        audit.quality_score * 100.0,
                        audit
                            .issues
                            .iter()
                            .map(|i| i.message.as_str())
                            .collect::<Vec<_>>()
                            .join("; ")
                    ),
                    suggestion: format!(
                        "Improve agent '{}' by addressing its issues.",
                        audit.agent.name
                    ),
                });
            }
        }
    }

    fn find_model_imbalance(report: &AuditReport, gaps: &mut Vec<Gap>) {
        let total = report.audits.len() as f64;
        if total == 0.0 {
            return;
        }

        let opus_count = report.model_distribution.get("opus").copied().unwrap_or(0) as f64;
        let sonnet_count = report
            .model_distribution
            .get("sonnet")
            .copied()
            .unwrap_or(0) as f64;

        let opus_ratio = opus_count / total;
        let sonnet_ratio = sonnet_count / total;

        if opus_ratio < 0.1 && sonnet_ratio > 0.5 {
            gaps.push(Gap {
                category: GapCategory::UnderrepresentedModel,
                severity: Severity::Medium,
                title: "Underuse of opus model".to_string(),
                description: format!(
                    "Only {:.0}% of agents use opus vs {:.0}% using sonnet. \
                     Complex agents (architect, planner) benefit from opus.",
                    opus_ratio * 100.0,
                    sonnet_ratio * 100.0
                ),
                suggestion: "Consider upgrading key agents (architect, planner, evolver) to opus."
                    .to_string(),
            });
        }

        if sonnet_ratio < 0.2 && opus_ratio > 0.5 && total > 5.0 {
            gaps.push(Gap {
                category: GapCategory::UnderrepresentedModel,
                severity: Severity::Low,
                title: "Overuse of opus model".to_string(),
                description: "Many agents use opus when sonnet would suffice.".to_string(),
                suggestion: "Consider downgrading routine agents to sonnet for cost efficiency."
                    .to_string(),
            });
        }
    }

    fn find_tool_coverage_gaps(report: &AuditReport, gaps: &mut Vec<Gap>) {
        let total = report.audits.len() as f64;
        if total == 0.0 {
            return;
        }

        let read_count = report.tool_distribution.get("Read").copied().unwrap_or(0) as f64;
        let _grep_count = report.tool_distribution.get("Grep").copied().unwrap_or(0) as f64;
        let _bash_count = report.tool_distribution.get("Bash").copied().unwrap_or(0) as f64;
        let _edit_count = report.tool_distribution.get("Edit").copied().unwrap_or(0) as f64;

        let no_tools: Vec<&str> = report
            .audits
            .iter()
            .filter(|a| !a.has_tools)
            .map(|a| a.agent.name.as_str())
            .collect();

        if !no_tools.is_empty() {
            gaps.push(Gap {
                category: GapCategory::IncompleteAgent,
                severity: Severity::High,
                title: format!("{} agents with no tools", no_tools.len()),
                description: format!(
                    "Agents without tools: {}. These agents cannot execute anything.",
                    no_tools.join(", ")
                ),
                suggestion: "Add appropriate tools to each agent definition.".to_string(),
            });
        }

        if read_count / total < 0.5 {
            gaps.push(Gap {
                category: GapCategory::IncompleteAgent,
                severity: Severity::Low,
                title: "Low Read tool adoption".to_string(),
                description: format!(
                    "Only {:.0}% of agents include 'Read'. Most agents need reading capability.",
                    read_count / total * 100.0
                ),
                suggestion: "Add Read to agent tool lists where appropriate.".to_string(),
            });
        }
    }

    fn find_coverage_duplicates(report: &AuditReport, gaps: &mut Vec<Gap>) {
        let mut seen: std::collections::HashMap<String, Vec<&str>> =
            std::collections::HashMap::new();

        for audit in &report.audits {
            let key = audit.agent.name.to_lowercase();
            seen.entry(key).or_default().push(audit.agent.name.as_str());
        }

        for (_key, names) in seen.iter().filter(|(_, v)| v.len() > 1) {
            gaps.push(Gap {
                category: GapCategory::DuplicateCoverage,
                severity: Severity::Medium,
                title: format!("Duplicate agents found: {}", names.join(", ")),
                description: format!(
                    "Multiple agents share the same normalized name: {}",
                    names.join(", ")
                ),
                suggestion: "Merge or rename to avoid confusion.".to_string(),
            });
        }
    }

    fn identify_bottlenecks(
        report: &AuditReport,
        bottlenecks: &mut Vec<Bottleneck>,
        _recommendations: &mut Vec<String>,
    ) {
        if report.audits.is_empty() {
            return;
        }

        let has_review_coverage = report
            .audits
            .iter()
            .any(|a| a.agent.name.contains("reviewer") || a.agent.name.contains("review"));
        if !has_review_coverage {
            bottlenecks.push(Bottleneck {
                area: "Code Review".to_string(),
                impact: "No dedicated code review agent exists. All agent output goes unchecked."
                    .to_string(),
                suggestion: "Create a code-reviewer agent and add review steps.".to_string(),
                severity: Severity::Critical,
            });
        }

        let has_tester = report
            .audits
            .iter()
            .any(|a| a.agent.name.contains("tester") || a.agent.name.contains("test"));
        if !has_tester {
            bottlenecks.push(Bottleneck {
                area: "Testing".to_string(),
                impact: "No dedicated testing agent. Agent quality cannot be verified.".to_string(),
                suggestion: "Create a tester agent or add test generation capability.".to_string(),
                severity: Severity::High,
            });
        }

        let no_model_count = report.audits.iter().filter(|a| !a.has_model).count();
        if no_model_count > 0 {
            let ratio = no_model_count as f64 / report.audits.len() as f64;
            if ratio > 0.2 {
                bottlenecks.push(Bottleneck {
                    area: "Model Configuration".to_string(),
                    impact: format!(
                        "{} agents ({}%) lack model specification. They use default model.",
                        no_model_count,
                        (ratio * 100.0) as u32
                    ),
                    suggestion: "Add explicit model fields to all agents.".to_string(),
                    severity: Severity::Medium,
                });
            }
        }

        let low_quality_count = report
            .audits
            .iter()
            .filter(|a| a.quality_score < 0.4)
            .count();
        if low_quality_count > 0 {
            let ratio = low_quality_count as f64 / report.audits.len() as f64;
            if ratio > 0.15 {
                bottlenecks.push(Bottleneck {
                    area: "Agent Quality".to_string(),
                    impact: format!(
                        "{} agents ({}%) have critically low quality scores (<40%).",
                        low_quality_count,
                        (ratio * 100.0) as u32
                    ),
                    suggestion: "Run the evolver to improve or replace these agents.".to_string(),
                    severity: Severity::High,
                });
            }
        }

        if report.ecosystem_health < 0.5 && report.agent_count > 5 {
            bottlenecks.push(Bottleneck {
                area: "Ecosystem Health".to_string(),
                impact: format!(
                    "Overall ecosystem health is {:.0}%, below 50% threshold.",
                    report.ecosystem_health * 100.0
                ),
                suggestion: "Systematic improvement needed across all agents.".to_string(),
                severity: Severity::High,
            });
        }
    }

    fn generate_recommendations(
        gaps: &[Gap],
        bottlenecks: &[Bottleneck],
        recommendations: &mut Vec<String>,
    ) {
        let critical_count = gaps
            .iter()
            .filter(|g| g.severity == Severity::Critical)
            .count();
        let high_count = gaps.iter().filter(|g| g.severity == Severity::High).count();
        let missing_lang_count = gaps
            .iter()
            .filter(|g| g.category == GapCategory::MissingLanguageAgent)
            .count();
        let low_quality_count = gaps
            .iter()
            .filter(|g| g.category == GapCategory::LowQualityAgent)
            .count();

        if critical_count > 0 {
            recommendations.push(format!(
                "Fix {} critical issues immediately",
                critical_count
            ));
        }
        if high_count > 0 {
            recommendations.push(format!("Address {} high-severity issues", high_count));
        }
        if missing_lang_count > 0 {
            recommendations.push(format!(
                "Generate {} missing language agents",
                missing_lang_count
            ));
        }
        if low_quality_count > 0 {
            recommendations.push(format!(
                "Improve or replace {} low-quality agents",
                low_quality_count
            ));
        }

        for bn in bottlenecks {
            recommendations.push(format!("Bottleneck [{}]: {}", bn.area, bn.suggestion));
        }
    }

    fn build_quality_histogram(report: &AuditReport) -> Vec<usize> {
        let mut hist = vec![0usize; 10];
        for audit in &report.audits {
            let bucket = ((audit.quality_score * 10.0) as usize).min(9);
            hist[bucket] += 1;
        }
        hist
    }

    pub fn format_analysis(result: &AnalysisResult) -> String {
        let mut out = String::new();
        use std::fmt::Write;

        let _ = writeln!(out, "╔══════════════════════════════════════════════╗");
        let _ = writeln!(out, "║           ECOSYSTEM GAP ANALYSIS             ║");
        let _ = writeln!(out, "╚══════════════════════════════════════════════╝");
        let _ = writeln!(out);

        let _ = writeln!(out, "── Gaps: {} found ──", result.gaps.len());
        for (i, gap) in result.gaps.iter().enumerate() {
            let sev = match gap.severity {
                Severity::Critical => "CRITICAL",
                Severity::High => "HIGH    ",
                Severity::Medium => "MEDIUM  ",
                Severity::Low => "LOW     ",
                Severity::Info => "INFO    ",
            };
            let _ = writeln!(out, "  {}. [{}] {}", i + 1, sev, gap.title);
            let _ = writeln!(out, "     {}", gap.description);
            let _ = writeln!(out, "     → {}", gap.suggestion);
            let _ = writeln!(out);
        }

        if !result.bottlenecks.is_empty() {
            let _ = writeln!(out, "── Bottlenecks (Amdahl's Law) ──");
            for (i, bn) in result.bottlenecks.iter().enumerate() {
                let sev = match bn.severity {
                    Severity::Critical => "CRITICAL",
                    Severity::High => "HIGH    ",
                    Severity::Medium => "MEDIUM  ",
                    Severity::Low => "LOW     ",
                    Severity::Info => "INFO    ",
                };
                let _ = writeln!(out, "  {}. [{}] {} — {}", i + 1, sev, bn.area, bn.impact);
                let _ = writeln!(out, "     → {}", bn.suggestion);
            }
            let _ = writeln!(out);
        }

        if !result.recommendations.is_empty() {
            let _ = writeln!(out, "── Recommendations ──");
            for rec in &result.recommendations {
                let _ = writeln!(out, "  • {}", rec);
            }
            let _ = writeln!(out);
        }

        let _ = writeln!(out, "── Quality Histogram (0-100%) ──");
        let max_count = result
            .quality_histogram
            .iter()
            .max()
            .copied()
            .unwrap_or(1)
            .max(1);
        for (bucket, &count) in result.quality_histogram.iter().enumerate() {
            let bar_len = (count * 20 / max_count).max(if count > 0 { 1 } else { 0 });
            let bar = "█".repeat(bar_len);
            let _ = writeln!(
                out,
                "  {:>3}% │ {} {}",
                bucket * 10,
                bar,
                if count > 0 {
                    format!("({})", count)
                } else {
                    String::new()
                }
            );
        }

        let _ = writeln!(out, "────────────────────────────────────────────");
        out
    }
}
