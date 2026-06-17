use crate::frontmatter;
use crate::types::*;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub struct Auditor;

impl Auditor {
    pub fn audit_agents(config: &EvolutionConfig) -> Vec<AgentAudit> {
        let dir = &config.agents_dir;
        if !dir.exists() || !dir.is_dir() {
            tracing::warn!(path = %dir.display(), "Agents directory not found");
            return Vec::new();
        }

        let mut audits = Vec::new();
        let walker = walkdir::WalkDir::new(dir).max_depth(1);
        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "md") {
                continue;
            }
            let agent_def = match Self::parse_agent_file(path) {
                Some(def) => def,
                None => continue,
            };
            let audit = Self::audit_agent(&agent_def);
            audits.push(audit);
        }

        audits.sort_by(|a, b| {
            b.quality_score
                .partial_cmp(&a.quality_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        audits
    }

    fn parse_agent_file(path: &Path) -> Option<AgentDef> {
        let content = fs::read_to_string(path).ok()?;

        let (fm, _consumed) = frontmatter::parse_frontmatter(&content)?;
        let body = frontmatter::extract_body(&content).to_string();
        let raw_fm = fm.raw.clone();

        Some(AgentDef {
            name: fm.get_string("name").unwrap_or_else(|| {
                path.file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default()
            }),
            description: fm.get_string("description").unwrap_or_default(),
            tools: fm.get_string_array("tools").unwrap_or_default(),
            model: fm.get_string("model").unwrap_or_default(),
            effort: fm.get_string("effort"),
            color: fm.get_string("color"),
            max_turns: fm.get_integer("maxturns").map(|i| i as usize),
            allowed_tools: fm.get_string_array("allowed_tools"),
            body,
            path: path.to_path_buf(),
            raw_frontmatter: raw_fm,
        })
    }

    fn audit_agent(agent: &AgentDef) -> AgentAudit {
        let mut issues = Vec::new();

        let has_valid_frontmatter = !agent.raw_frontmatter.is_empty();
        if !has_valid_frontmatter {
            issues.push(Issue {
                severity: Severity::Critical,
                category: IssueCategory::InvalidFrontmatter,
                message: format!("Agent {} has no valid YAML frontmatter", agent.name),
            });
        }

        let has_name = !agent.name.is_empty();
        if !has_name {
            issues.push(Issue {
                severity: Severity::Critical,
                category: IssueCategory::MissingField,
                message: "Agent is missing a name field".to_string(),
            });
        }

        let has_description = !agent.description.is_empty();
        if !has_description {
            issues.push(Issue {
                severity: Severity::High,
                category: IssueCategory::MissingField,
                message: format!("Agent {} is missing a description", agent.name),
            });
        }

        let desc_len = agent.description.len();
        let description_quality = DescriptionQuality::from_len(desc_len);
        match description_quality {
            DescriptionQuality::Missing | DescriptionQuality::TooShort => {
                issues.push(Issue {
                    severity: Severity::High,
                    category: IssueCategory::PoorDescription,
                    message: format!(
                        "Agent {} has a poor description ({} chars, needs >= 20)",
                        agent.name, desc_len
                    ),
                });
            }
            _ => {}
        }

        let has_tools = !agent.tools.is_empty();
        if !has_tools {
            issues.push(Issue {
                severity: Severity::High,
                category: IssueCategory::MissingTools,
                message: format!("Agent {} has no tools specified", agent.name),
            });
        }

        let has_model = !agent.model.is_empty();
        if !has_model {
            issues.push(Issue {
                severity: Severity::Medium,
                category: IssueCategory::MissingField,
                message: format!("Agent {} has no model specified", agent.name),
            });
        } else if !KNOWN_MODELS.contains(&agent.model.as_str()) {
            issues.push(Issue {
                severity: Severity::Low,
                category: IssueCategory::UnknownModel,
                message: format!("Agent {} uses unknown model '{}'", agent.name, agent.model),
            });
        }

        let body_length = agent.body.len();
        if body_length < 50 {
            issues.push(Issue {
                severity: Severity::Medium,
                category: IssueCategory::EmptyBody,
                message: format!(
                    "Agent {} has very short body ({} chars)",
                    agent.name, body_length
                ),
            });
        }

        let name_check = check_naming_convention(&agent.name);
        if let Some(msg) = name_check {
            issues.push(Issue {
                severity: Severity::Low,
                category: IssueCategory::NamingConvention,
                message: msg,
            });
        }

        let quality_score = compute_quality_score(
            has_valid_frontmatter,
            has_name,
            has_description,
            &description_quality,
            has_tools,
            has_model,
            body_length,
            &issues,
        );

        AgentAudit {
            agent: agent.clone(),
            has_valid_frontmatter,
            has_name,
            has_description,
            description_quality,
            has_tools,
            has_model,
            body_length,
            issues,
            quality_score,
        }
    }

    pub fn generate_report(
        _config: &EvolutionConfig,
        audits: &[AgentAudit],
        other_counts: &AgentFileCounts,
    ) -> AuditReport {
        let mut language_coverage: HashMap<String, usize> = HashMap::new();
        let mut model_distribution: HashMap<String, usize> = HashMap::new();
        let mut tool_distribution: HashMap<String, usize> = HashMap::new();

        for audit in audits {
            for lang in KNOWN_LANGUAGES {
                if audit.agent.name.to_lowercase().contains(lang)
                    || audit.agent.description.to_lowercase().contains(lang)
                {
                    *language_coverage.entry(lang.to_string()).or_insert(0) += 1;
                }
            }

            if audit.has_model {
                *model_distribution
                    .entry(audit.agent.model.clone())
                    .or_insert(0) += 1;
            }

            for tool in &audit.agent.tools {
                *tool_distribution.entry(tool.clone()).or_insert(0) += 1;
            }
        }

        let total = audits.len() as f64;
        let ecosystem_health = if total > 0.0 {
            audits.iter().map(|a| a.quality_score).sum::<f64>() / total
        } else {
            0.0
        };

        let mut scan_errors = Vec::new();
        if audits.is_empty() {
            scan_errors.push("No agent files found or parsed".to_string());
        }

        AuditReport {
            timestamp: chrono::Utc::now(),
            agent_count: audits.len(),
            command_count: other_counts.commands,
            skill_count: other_counts.skills,
            rule_count: other_counts.rules,
            audits: audits.to_vec(),
            ecosystem_health,
            language_coverage,
            model_distribution,
            tool_distribution,
            scan_errors,
        }
    }

    pub fn count_files(config: &EvolutionConfig) -> AgentFileCounts {
        AgentFileCounts {
            commands: count_md_files(&config.commands_dir),
            skills: count_skill_dirs(&config.skills_dir),
            rules: count_md_files(&config.rules_dir),
        }
    }
}

fn count_md_files(dir: &Path) -> usize {
    if !dir.exists() {
        return 0;
    }
    walkdir::WalkDir::new(dir)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        .count()
}

fn count_skill_dirs(dir: &Path) -> usize {
    if !dir.exists() {
        return 0;
    }
    walkdir::WalkDir::new(dir)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter(|e| {
            let name = e.file_name().to_string_lossy();
            name != "." && !name.starts_with('.')
        })
        .count()
}

#[derive(Debug, Clone)]
pub struct AgentFileCounts {
    pub commands: usize,
    pub skills: usize,
    pub rules: usize,
}

fn check_naming_convention(name: &str) -> Option<String> {
    if name.contains('_') {
        return Some(format!(
            "Agent '{}' uses underscores in name; kebab-case preferred",
            name
        ));
    }
    if name.contains(' ') {
        return Some(format!(
            "Agent '{}' contains spaces in name; kebab-case preferred",
            name
        ));
    }
    if name.chars().any(|c| c.is_uppercase()) && name.len() > 1 {
        let lower_count = name.chars().filter(|c| c.is_lowercase()).count();
        if lower_count > 0 {
            return Some(format!(
                "Agent '{}' uses mixed case; kebab-case preferred",
                name
            ));
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn compute_quality_score(
    has_valid_fm: bool,
    has_name: bool,
    has_description: bool,
    desc_quality: &DescriptionQuality,
    has_tools: bool,
    has_model: bool,
    body_length: usize,
    issues: &[Issue],
) -> f64 {
    let mut score = 0.0;

    if has_valid_fm {
        score += 0.15;
    }
    if has_name {
        score += 0.10;
    }
    if has_description {
        score += 0.10;
    }
    score += desc_quality.score() * 0.15;
    if has_tools {
        score += 0.15;
    }
    if has_model {
        score += 0.10;
    }

    let body_score = (body_length as f64 / 500.0).min(1.0);
    score += body_score * 0.10;

    let max_penalty = issues.len() as f64 * 0.05;
    score = (score - max_penalty).max(0.0);

    score.min(1.0)
}

pub fn format_audit_report(report: &AuditReport) -> String {
    let mut out = String::new();
    use std::fmt::Write;

    let _ = writeln!(out, "╔══════════════════════════════════════════════╗");
    let _ = writeln!(out, "║        AGENT ECOSYSTEM AUDIT REPORT          ║");
    let _ = writeln!(out, "╚══════════════════════════════════════════════╝");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "  Timestamp:    {}",
        report.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
    );
    let _ = writeln!(out, "  Agents:       {}", report.agent_count);
    let _ = writeln!(out, "  Commands:     {}", report.command_count);
    let _ = writeln!(out, "  Skills:       {}", report.skill_count);
    let _ = writeln!(out, "  Rules:        {}", report.rule_count);
    let _ = writeln!(
        out,
        "  Health Score: {:.1}%",
        report.ecosystem_health * 100.0
    );
    let _ = writeln!(out);

    let _ = writeln!(out, "── Agent Quality Distribution ──");
    let mut buckets = [0usize; 4];
    for audit in &report.audits {
        let q = audit.quality_score;
        if q >= 0.8 {
            buckets[0] += 1;
        } else if q >= 0.6 {
            buckets[1] += 1;
        } else if q >= 0.4 {
            buckets[2] += 1;
        } else {
            buckets[3] += 1;
        }
    }
    let _ = writeln!(out, "  Excellent (≥80%):  {}", buckets[0]);
    let _ = writeln!(out, "  Good (60-79%):     {}", buckets[1]);
    let _ = writeln!(out, "  Fair (40-59%):     {}", buckets[2]);
    let _ = writeln!(out, "  Poor (<40%):       {}", buckets[3]);
    let _ = writeln!(out);

    if !report.model_distribution.is_empty() {
        let _ = writeln!(out, "── Model Distribution ──");
        for (model, count) in &report.model_distribution {
            let _ = writeln!(out, "  {:<12}: {}", model, count);
        }
        let _ = writeln!(out);
    }

    if !report.tool_distribution.is_empty() {
        let _ = writeln!(out, "── Most Used Tools ──");
        let mut tools: Vec<_> = report.tool_distribution.iter().collect();
        tools.sort_by(|a, b| b.1.cmp(a.1));
        for (tool, count) in tools.iter().take(8) {
            let _ = writeln!(out, "  {:<12}: {}", tool, count);
        }
        let _ = writeln!(out);
    }

    if !report.language_coverage.is_empty() {
        let _ = writeln!(out, "── Language Coverage ──");
        let mut langs: Vec<_> = report.language_coverage.iter().collect();
        langs.sort_by(|a, b| b.1.cmp(a.1));
        for (lang, count) in langs.iter().take(10) {
            let _ = writeln!(out, "  {:<12}: {} agent(s)", lang, count);
        }
        let _ = writeln!(out);
    }

    let issues_total: usize = report.audits.iter().map(|a| a.issues.len()).sum();
    if issues_total > 0 {
        let _ = writeln!(out, "── Top Issues ──");
        let mut all_issues: Vec<&Issue> =
            report.audits.iter().flat_map(|a| a.issues.iter()).collect();
        all_issues.sort_by(|a, b| {
            b.severity
                .score()
                .partial_cmp(&a.severity.score())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for issue in all_issues.iter().take(5) {
            let severity_str = match issue.severity {
                Severity::Critical => "CRIT",
                Severity::High => "HIGH ",
                Severity::Medium => "MED  ",
                Severity::Low => "LOW  ",
                Severity::Info => "INFO ",
            };
            let _ = writeln!(out, "  [{}] {}", severity_str, issue.message);
        }
        let _ = writeln!(out, "  ({} total issues)", issues_total);
    }

    if !report.scan_errors.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "── Scan Errors ──");
        for err in &report.scan_errors {
            let _ = writeln!(out, "  ⚠ {}", err);
        }
    }

    let _ = writeln!(out, "────────────────────────────────────────────");
    out
}
