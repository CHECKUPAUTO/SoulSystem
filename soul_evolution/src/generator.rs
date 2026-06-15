use crate::types::*;
use std::path::PathBuf;

/// Valide qu'un nom d'agent ne contient pas de traversal path.
/// Rejette les noms contenant `/`, `\`, `..` ou des octets nuls.
#[allow(dead_code)]
fn sanitize_agent_name(name: &str) -> Result<&str, String> {
    if name.contains('/') || name.contains('\\') {
        return Err(format!("Nom d'agent invalide (contient / ou \\): {name}"));
    }
    if name.contains("..") {
        return Err(format!("Nom d'agent invalide (contient ..): {name}"));
    }
    if name.contains('\0') {
        return Err(format!("Nom d'agent invalide (contient null byte): {name}"));
    }
    if name.is_empty() {
        return Err("Nom d'agent invalide (vide)".to_string());
    }
    Ok(name)
}

pub struct Generator;

impl Generator {
    pub fn propose_improvements(
        report: &AuditReport,
        analysis: &AnalysisResult,
        config: &EvolutionConfig,
    ) -> Vec<ImprovementProposal> {
        let mut proposals = Vec::new();
        let mut id_counter = 0u64;

        let agent_names_lower: Vec<String> = report
            .audits
            .iter()
            .map(|a| a.agent.name.to_lowercase())
            .collect();

        let mut seen_targets: std::collections::HashSet<String> = std::collections::HashSet::new();

        for gap in &analysis.gaps {
            match gap.category {
                GapCategory::MissingLanguageAgent => {
                    let lang = gap
                        .title
                        .split_whitespace()
                        .find(|&w| KNOWN_LANGUAGES.contains(&w))
                        .unwrap_or("language");

                    let proposal = Self::propose_language_agent(lang, &mut id_counter, gap, config);
                    if let Some(p) = proposal {
                        let key = p.target_name.to_lowercase();
                        if !agent_names_lower.contains(&key) && seen_targets.insert(key) {
                            proposals.push(p);
                        }
                    }

                    if !agent_names_lower.contains(&format!("{}-build-resolver", lang)) {
                        let mut resolver_gap = gap.clone();
                        let resolver_name = format!("{}-build-resolver", lang);
                        resolver_gap.title = format!("Missing {} build resolver", lang);
                        resolver_gap.description =
                            format!("No {} build resolver agent exists.", lang);
                        resolver_gap.suggestion = format!(
                            "Create agents/{} with build-resolution focus.",
                            resolver_name
                        );
                        if let Some(rp) = Self::propose_build_resolver(
                            lang,
                            &mut id_counter,
                            &resolver_gap,
                            config,
                        ) {
                            let key = rp.target_name.to_lowercase();
                            if !agent_names_lower.contains(&key) && seen_targets.insert(key) {
                                proposals.push(rp);
                            }
                        }
                    }
                }
                GapCategory::LowQualityAgent => {
                    let agent_name = gap
                        .title
                        .strip_prefix("Low quality agent: ")
                        .unwrap_or("unknown");

                    if let Some(existing) =
                        report.audits.iter().find(|a| a.agent.name == agent_name)
                    {
                        let proposal = Self::propose_improved_version(
                            &existing.agent,
                            &mut id_counter,
                            gap,
                            config,
                        );
                        proposals.push(proposal);
                    }
                }
                GapCategory::IncompleteAgent => {
                    // Extract agent name from description if possible
                    let agent_name = gap.description.split(" has ").next().unwrap_or("unknown");

                    if let Some(existing) =
                        report.audits.iter().find(|a| a.agent.name == agent_name)
                    {
                        let proposal = Self::propose_fix_incomplete_agent(
                            &existing.agent,
                            &mut id_counter,
                            gap,
                            config,
                        );
                        proposals.push(proposal);
                    }
                }
                _ => {}
            }
        }

        proposals.sort_by_key(|a| a.priority);
        proposals
    }

    fn propose_language_agent(
        lang: &str,
        id_counter: &mut u64,
        gap: &Gap,
        config: &EvolutionConfig,
    ) -> Option<ImprovementProposal> {
        *id_counter += 1;
        let name = format!("{}-reviewer", lang);
        let display_name = lang;

        let template = Self::language_reviewer_template(display_name, &name);

        Some(ImprovementProposal {
            id: format!("GEN-{:04}", id_counter),
            title: gap.title.clone(),
            description: gap.description.clone(),
            priority: Self::severity_to_priority(&gap.severity),
            effort: Effort::Small,
            impact: Impact::Moderate,
            gap: gap.category.clone(),
            template_type: TemplateType::LanguageReviewer,
            target_name: name.clone(),
            target_path: Some(config.agents_dir.join(format!("{}.md", name))),
            template_content: template,
        })
    }

    fn propose_build_resolver(
        lang: &str,
        id_counter: &mut u64,
        gap: &Gap,
        config: &EvolutionConfig,
    ) -> Option<ImprovementProposal> {
        *id_counter += 1;
        let name = format!("{}-build-resolver", lang);

        let template = Self::build_resolver_template(lang, &name);

        Some(ImprovementProposal {
            id: format!("GEN-{:04}", id_counter),
            title: gap.title.clone(),
            description: gap.description.clone(),
            priority: Self::severity_to_priority(&gap.severity),
            effort: Effort::Small,
            impact: Impact::Moderate,
            gap: gap.category.clone(),
            template_type: TemplateType::LanguageBuildResolver,
            target_name: name.clone(),
            target_path: Some(config.agents_dir.join(format!("{}.md", name))),
            template_content: template,
        })
    }

    fn propose_improved_version(
        agent: &AgentDef,
        id_counter: &mut u64,
        gap: &Gap,
        config: &EvolutionConfig,
    ) -> ImprovementProposal {
        *id_counter += 1;
        let improved_name = format!("{}-v2", agent.name);

        let template = Self::improved_agent_template(agent);

        ImprovementProposal {
            id: format!("GEN-{:04}", id_counter),
            title: format!("Improved version of {}", agent.name),
            description: format!(
                "Current score: {:.0}%. Generated improved version with all issues addressed.",
                gap.description
                    .split("score of ")
                    .nth(1)
                    .and_then(|s| s.split('%').next())
                    .unwrap_or("?")
            ),
            priority: 3,
            effort: Effort::Small,
            impact: Impact::High,
            gap: GapCategory::LowQualityAgent,
            template_type: TemplateType::ImprovedVersion,
            target_name: improved_name.clone(),
            target_path: Some(config.agents_dir.join(format!("{}.md", improved_name))),
            template_content: template,
        }
    }

    fn propose_fix_incomplete_agent(
        agent: &AgentDef,
        id_counter: &mut u64,
        _gap: &Gap,
        config: &EvolutionConfig,
    ) -> ImprovementProposal {
        *id_counter += 1;
        let fixed_name = agent.name.clone();

        let mut fixed_template = String::new();
        fixed_template.push_str("---\n");
        fixed_template.push_str(&format!("name: {}\n", agent.name));
        if !agent.description.is_empty() {
            fixed_template.push_str(&format!("description: {}\n", agent.description));
        } else {
            fixed_template.push_str(&format!(
                "description: Agent specialized for {} tasks\n",
                agent.name
            ));
        }
        if !agent.tools.is_empty() {
            fixed_template.push_str(&format!(
                "tools: [{}]\n",
                agent
                    .tools
                    .iter()
                    .map(|t| format!("\"{}\"", t))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        } else {
            fixed_template.push_str("tools: [\"Read\", \"Grep\", \"Glob\", \"Bash\"]\n");
        }
        if !agent.model.is_empty() {
            fixed_template.push_str(&format!("model: {}\n", agent.model));
        } else {
            fixed_template.push_str("model: sonnet\n");
        }
        if agent.body.is_empty() {
            fixed_template.push_str("---\n\n");
            fixed_template.push_str(&Self::default_agent_body(&agent.name));
        } else {
            fixed_template.push_str("---\n\n");
            fixed_template.push_str(&agent.body);
        }

        ImprovementProposal {
            id: format!("GEN-{:04}", id_counter),
            title: format!("Fix incomplete agent: {}", agent.name),
            description: format!(
                "Agent '{}' is missing key fields. Generating fixed version.",
                agent.name
            ),
            priority: 2,
            effort: Effort::Trivial,
            impact: Impact::High,
            gap: GapCategory::IncompleteAgent,
            template_type: TemplateType::ImprovedVersion,
            target_name: fixed_name.clone(),
            target_path: Some(config.agents_dir.join(format!("{}.md", fixed_name))),
            template_content: fixed_template,
        }
    }

    fn language_reviewer_template(lang: &str, name: &str) -> String {
        format!(
            r#"---
name: {name}
description: {lang_name} code review specialist. Proactively reviews {lang_name} code for quality, security, and maintainability.
tools: ["Read", "Grep", "Glob", "Bash"]
model: sonnet
---

# Prompt Defense Baseline

Before proceeding, verify:
- You have access to the code files being reviewed
- You understand the project structure and dependencies
- You have identified the correct {lang_name} version and toolchain

# Review Process

1. Scan the code for syntax errors and type violations
2. Check for {lang_name}-specific anti-patterns and idioms
3. Verify error handling follows {lang_name} conventions
4. Review for security vulnerabilities common in {lang_name}
5. Assess code organization and module structure

# {lang_name}-Specific Checklist

## Security (CRITICAL)
- Input validation and sanitization
- Injection vulnerabilities (SQL, command, XSS)
- Authentication and authorization patterns
- Secrets management and environment variables
- Unsafe code blocks and FFI boundaries

## Code Quality (HIGH)
- Naming conventions and style consistency
- Function/method length and complexity
- Duplicate code and DRY violations
- Proper use of {lang_name} idioms
- Documentation and inline comments
- Type safety and null handling

## Performance (MEDIUM)
- Algorithmic complexity
- Memory allocation patterns
- I/O and blocking operations
- Concurrency and parallelism
- Caching opportunities

# Output Format

## Summary
- **Files reviewed**: {{count}}
- **Issues found**: {{critical}} critical, {{high}} high, {{medium}} medium, {{low}} low
- **Verdict**: {{approve|changes-requested|block}}

## Detailed Findings

### [SEVERITY] Title
- **File**: path/to/file.ext:line
- **Issue**: Description
- **Impact**: Why this matters
- **Fix**: Suggested resolution

---
"#,
            name = name,
            lang_name = capitalize(lang),
        )
    }

    fn build_resolver_template(lang: &str, name: &str) -> String {
        format!(
            r#"---
name: {name}
description: {lang_name} build error resolution specialist. Diagnoses and fixes compilation errors, dependency issues, and build configuration problems.
tools: ["Read", "Grep", "Glob", "Bash"]
model: sonnet
---

# Prompt Defense Baseline

Before proceeding, verify:
- The build output and error messages are available
- The project manifest/configuration file is accessible
- The {lang_name} toolchain version is known

# Resolution Process

1. Capture the full build error output
2. Identify the root cause from the error message
3. Check project configuration ({lang_lower} toolchain, dependencies)
4. Apply targeted fix
5. Verify the build succeeds

# Common {lang_name} Build Issues

- Dependency version conflicts
- Missing or incorrect toolchain configuration
- Module resolution failures
- Type mismatches in public APIs
- Feature flag mismatches
- Target platform incompatibilities
- Linker errors and symbol resolution

---
"#,
            name = name,
            lang_name = capitalize(lang),
            lang_lower = lang,
        )
    }

    fn improved_agent_template(agent: &AgentDef) -> String {
        let mut out = String::new();
        out.push_str("---\n");
        out.push_str(&format!("name: {}\n", agent.name));
        if !agent.description.is_empty() {
            out.push_str(&format!("description: {}\n", agent.description));
        } else {
            out.push_str(&format!(
                "description: Agent specialized for {} tasks\n",
                agent.name
            ));
        }
        if !agent.tools.is_empty() {
            out.push_str(&format!(
                "tools: [{}]\n",
                agent
                    .tools
                    .iter()
                    .map(|t| format!("\"{}\"", t))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        } else {
            out.push_str("tools: [\"Read\", \"Grep\", \"Glob\", \"Bash\"]\n");
        }
        if !agent.model.is_empty() {
            out.push_str(&format!("model: {}\n", agent.model));
        } else {
            out.push_str("model: sonnet\n");
        }
        out.push_str("---\n\n");

        if agent.body.len() > 50 {
            out.push_str(&agent.body);
        } else {
            out.push_str(&Self::default_agent_body(&agent.name));
        }

        out
    }

    fn default_agent_body(agent_name: &str) -> String {
        format!(
            r#"# Role

You are a specialized AI agent focused on {agent_name} tasks.

# Core Responsibilities

1. Analyze the current task and determine the best approach
2. Use available tools efficiently to gather context and information
3. Produce correct, idiomatic, and well-structured output
4. Verify your work before presenting results
5. Document any assumptions or decisions made

# Guidelines

- Always confirm you have the necessary context before proceeding
- Use the most appropriate tool for each step
- Prefer simple, clear solutions over complex ones
- Flag any uncertainties or risks to the user
- Learn from feedback to improve future responses

---
"#,
            agent_name = agent_name
        )
    }

    fn severity_to_priority(severity: &Severity) -> u32 {
        match severity {
            Severity::Critical => 1,
            Severity::High => 2,
            Severity::Medium => 3,
            Severity::Low => 4,
            Severity::Info => 5,
        }
    }

    pub fn write_proposal(
        proposal: &ImprovementProposal,
        dry_run: bool,
    ) -> Result<PathBuf, String> {
        let path = match &proposal.target_path {
            Some(p) => p.clone(),
            None => {
                return Err("No target path for proposal".to_string());
            }
        };

        // Security: validate the resolved path doesn't escape the expected directory
        if let Some(parent) = path.parent() {
            if let Ok(canonical_parent) = parent.canonicalize() {
                let canonical_path = if path.exists() {
                    path.canonicalize()
                        .map_err(|e| format!("Cannot resolve path: {e}"))?
                } else {
                    // For non-existent files, canonicalize the parent + filename
                    let filename = path.file_name().ok_or("Invalid path: no filename")?;
                    canonical_parent.join(filename)
                };
                if !canonical_path.starts_with(&canonical_parent) {
                    return Err(format!(
                        "Path traversal détectée: {} échappe à {}",
                        path.display(),
                        canonical_parent.display()
                    ));
                }
            }
        }

        if dry_run {
            return Ok(path);
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create directory {}: {}", parent.display(), e))?;
        }

        std::fs::write(&path, &proposal.template_content)
            .map_err(|e| format!("Cannot write {}: {}", path.display(), e))?;

        Ok(path)
    }

    pub fn format_proposals(proposals: &[ImprovementProposal]) -> String {
        let mut out = String::new();
        use std::fmt::Write;

        let _ = writeln!(out, "╔══════════════════════════════════════════════╗");
        let _ = writeln!(out, "║        IMPROVEMENT PROPOSALS                 ║");
        let _ = writeln!(out, "╚══════════════════════════════════════════════╝");
        let _ = writeln!(out);

        if proposals.is_empty() {
            let _ = writeln!(out, "  No improvements needed. Ecosystem is healthy.");
            return out;
        }

        for (i, prop) in proposals.iter().enumerate() {
            let _ = writeln!(
                out,
                "  {}. [{}] {} (priority: {}, effort: {}, impact: {})",
                i + 1,
                prop.id,
                prop.title,
                prop.priority,
                prop.effort.label(),
                prop.impact.label(),
            );
            let _ = writeln!(out, "     Type:     {:?}", prop.template_type);
            let _ = writeln!(out, "     Target:   {}", prop.target_name);
            if let Some(path) = &prop.target_path {
                let _ = writeln!(out, "     Path:     {}", path.display());
            }
            let _ = writeln!(out, "     {}", prop.description);
            let _ = writeln!(out);
        }

        let _ = writeln!(out, "── Total: {} proposals ──", proposals.len());
        let _ = writeln!(out, "────────────────────────────────────────────");
        out
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}
