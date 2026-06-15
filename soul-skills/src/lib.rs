use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::fs;

pub mod induction;
pub mod validation;
pub use induction::{Episode, EpisodeAction, InducerConfig, Induction, SkillInducer};
pub use validation::{
    Retention, SkillFitness, SkillValidator, StructuralValidator, ValidatedSkillLibrary,
};

// ─── Errors ──────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum SkillError {
    #[error("IO error: {0}")]
    Io(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Skill not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, SkillError>;

// ─── Skill Definition ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub triggers: Vec<String>,
    #[serde(default)]
    pub tools_required: Vec<String>,
    #[serde(default)]
    pub model_hint: Option<String>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub steps: Vec<String>,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub enabled: bool,
}

impl Skill {
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            triggers: Vec::new(),
            tools_required: Vec::new(),
            model_hint: None,
            system_prompt: None,
            steps: Vec::new(),
            examples: Vec::new(),
            tags: Vec::new(),
            priority: 5,
            enabled: true,
        }
    }

    pub fn matches_trigger(&self, input: &str) -> bool {
        let input_lower = input.to_lowercase();
        self.triggers
            .iter()
            .any(|t| input_lower.contains(&t.to_lowercase()))
    }

    pub fn to_prompt(&self) -> String {
        let mut parts = vec![format!("# Skill: {}\n{}", self.name, self.description)];

        if !self.steps.is_empty() {
            parts.push("## Steps\n".to_string());
            for (i, step) in self.steps.iter().enumerate() {
                parts.push(format!("{}. {}", i + 1, step));
            }
        }

        if !self.examples.is_empty() {
            parts.push("## Examples\n".to_string());
            for ex in &self.examples {
                parts.push(format!("- {ex}"));
            }
        }

        if let Some(ref prompt) = self.system_prompt {
            parts.push(format!("## System Prompt\n{prompt}"));
        }

        parts.join("\n\n")
    }
}

// ─── Skill Loader ────────────────────────────────────────────────────────────

pub struct SkillLoader {
    base_path: PathBuf,
    skills: HashMap<String, Skill>,
}

impl SkillLoader {
    pub fn new(base_path: &Path) -> Self {
        Self {
            base_path: base_path.to_path_buf(),
            skills: HashMap::new(),
        }
    }

    pub async fn load_all(&mut self) -> Result<Vec<Skill>> {
        let mut skills = Vec::new();

        if !self.base_path.exists() {
            fs::create_dir_all(&self.base_path)
                .await
                .map_err(|e| SkillError::Io(e.to_string()))?;
            return Ok(skills);
        }

        let mut entries = fs::read_dir(&self.base_path)
            .await
            .map_err(|e| SkillError::Io(e.to_string()))?;

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                if let Ok(skill) = self.load_skill_file(&path).await {
                    skills.push(skill.clone());
                    self.skills.insert(skill.name.clone(), skill);
                }
            }
        }

        Ok(skills)
    }

    pub async fn load_skill_file(&self, path: &Path) -> Result<Skill> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| SkillError::Io(e.to_string()))?;

        self.parse_markdown_skill(&content, path)
    }

    fn parse_markdown_skill(&self, content: &str, path: &Path) -> Result<Skill> {
        let mut name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let mut description = String::new();
        let mut triggers = Vec::new();
        let mut tools_required = Vec::new();
        let mut steps = Vec::new();
        let mut examples = Vec::new();
        let mut system_prompt = None;
        let mut tags = Vec::new();
        let mut priority: u8 = 5;
        let mut in_steps = false;
        let mut in_examples = false;
        let mut has_header = false;

        for line in content.lines() {
            let trimmed = line.trim();

            if let Some(h) = trimmed.strip_prefix("# ") {
                name = h.trim().to_string();
                has_header = true;
                in_steps = false;
                in_examples = false;
            } else if let Some(h) = trimmed.strip_prefix("## ") {
                let section = h.trim().to_lowercase();
                in_steps = section == "steps" || section == "workflow";
                in_examples = section == "examples";
                if !in_steps && !in_examples {
                    // Check for YAML frontmatter-style metadata
                    if section.starts_with("description:") {
                        description = section
                            .trim_start_matches("description:")
                            .trim()
                            .to_string();
                    } else if section.starts_with("triggers:") {
                        triggers = section
                            .trim_start_matches("triggers:")
                            .trim()
                            .split(',')
                            .map(|s| s.trim().trim_matches('"').to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    } else if section.starts_with("tools:") {
                        tools_required = section
                            .trim_start_matches("tools:")
                            .trim()
                            .split(',')
                            .map(|s| s.trim().trim_matches('"').to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    } else if section.starts_with("tags:") {
                        tags = section
                            .trim_start_matches("tags:")
                            .trim()
                            .split(',')
                            .map(|s| s.trim().trim_matches('"').to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    } else if section.starts_with("priority:") {
                        priority = section
                            .trim_start_matches("priority:")
                            .trim()
                            .parse()
                            .unwrap_or(5);
                    }
                }
            } else if in_steps && !trimmed.is_empty() && !trimmed.starts_with('#') {
                steps.push(trimmed.to_string());
            } else if in_examples && !trimmed.is_empty() && !trimmed.starts_with('#') {
                examples.push(
                    trimmed
                        .trim_start_matches("- ")
                        .trim_start_matches("* ")
                        .to_string(),
                );
            } else if trimmed.starts_with("Description:") || trimmed.starts_with("description:") {
                description = trimmed
                    .split_once(':')
                    .map(|(_, v)| v)
                    .unwrap_or("")
                    .trim()
                    .to_string();
            } else if trimmed.starts_with("Triggers:") || trimmed.starts_with("triggers:") {
                triggers = trimmed
                    .split_once(':')
                    .map(|(_, v)| v)
                    .unwrap_or("")
                    .trim()
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            } else if trimmed.starts_with("Tools:") || trimmed.starts_with("tools:") {
                tools_required = trimmed
                    .split_once(':')
                    .map(|(_, v)| v)
                    .unwrap_or("")
                    .trim()
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            } else if trimmed.starts_with("Tags:") || trimmed.starts_with("tags:") {
                tags = trimmed
                    .split_once(':')
                    .map(|(_, v)| v)
                    .unwrap_or("")
                    .trim()
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            } else if trimmed.starts_with("Priority:") || trimmed.starts_with("priority:") {
                priority = trimmed
                    .split_once(':')
                    .map(|(_, v)| v)
                    .unwrap_or("")
                    .trim()
                    .parse()
                    .unwrap_or(5);
            } else if trimmed.starts_with("System Prompt:") || trimmed.starts_with("system_prompt:")
            {
                system_prompt = Some(
                    trimmed
                        .split_once(':')
                        .map(|(_, v)| v)
                        .unwrap_or("")
                        .trim()
                        .to_string(),
                );
            }
        }

        // Validation: warn if skill file is missing required structure
        if !has_header {
            tracing::warn!(path = %path.display(), "Skill file missing '# Name' header");
        }
        if description.is_empty() {
            description = format!("Skill: {name}");
            tracing::debug!(name, "Skill missing description, using fallback");
        }
        if triggers.is_empty() {
            tracing::warn!(name, "Skill has no triggers, will only match by name");
        }

        Ok(Skill {
            name,
            description,
            triggers,
            tools_required,
            model_hint: None,
            system_prompt,
            steps,
            examples,
            tags,
            priority,
            enabled: true,
        })
    }

    pub fn get_skill(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    pub fn find_matching(&self, input: &str) -> Vec<&Skill> {
        let mut matches: Vec<&Skill> = self
            .skills
            .values()
            .filter(|s| s.enabled && s.matches_trigger(input))
            .collect();
        matches.sort_by_key(|s| std::cmp::Reverse(s.priority));
        matches
    }

    pub fn save_skill(&self, skill: &Skill) -> impl std::future::Future<Output = Result<()>> + '_ {
        let skill = skill.clone();
        let path = self.base_path.join(format!("{}.md", skill.name));
        async move {
            let content = self.skill_to_markdown(&skill);
            fs::write(&path, content)
                .await
                .map_err(|e| SkillError::Io(e.to_string()))?;
            Ok(())
        }
    }

    fn skill_to_markdown(&self, skill: &Skill) -> String {
        let mut md = format!("# {}\n\n", skill.name);

        if !skill.description.is_empty() {
            md.push_str(&format!("Description: {}\n\n", skill.description));
        }

        if !skill.triggers.is_empty() {
            md.push_str(&format!("Triggers: {}\n\n", skill.triggers.join(", ")));
        }

        if !skill.tools_required.is_empty() {
            md.push_str(&format!("Tools: {}\n\n", skill.tools_required.join(", ")));
        }

        if !skill.tags.is_empty() {
            md.push_str(&format!("Tags: {}\n\n", skill.tags.join(", ")));
        }

        md.push_str(&format!("Priority: {}\n\n", skill.priority));

        if !skill.steps.is_empty() {
            md.push_str("## Steps\n\n");
            for (i, step) in skill.steps.iter().enumerate() {
                md.push_str(&format!("{}. {}\n", i + 1, step));
            }
            md.push('\n');
        }

        if !skill.examples.is_empty() {
            md.push_str("## Examples\n\n");
            for ex in &skill.examples {
                md.push_str(&format!("- {ex}\n"));
            }
            md.push('\n');
        }

        md
    }
}

// ─── Pre-built Skills ────────────────────────────────────────────────────────

pub fn builtin_skills() -> Vec<Skill> {
    vec![
        Skill {
            name: "code-review".into(),
            description: "Review code for bugs, security, and quality".into(),
            triggers: vec![
                "review".into(),
                "code review".into(),
                "check code".into(),
                "audit code".into(),
            ],
            tools_required: vec!["read_file".into(), "grep".into()],
            steps: vec![
                "Read the file(s) to review".into(),
                "Check for bugs and logic errors".into(),
                "Check for security issues".into(),
                "Check for code quality and style".into(),
                "Provide structured feedback".into(),
            ],
            tags: vec!["code".into(), "review".into(), "quality".into()],
            priority: 8,
            enabled: true,
            ..Skill::new("code-review", "Review code for bugs, security, and quality")
        },
        Skill {
            name: "deploy".into(),
            description: "Deploy code to production".into(),
            triggers: vec!["deploy".into(), "ship".into(), "push to prod".into()],
            tools_required: vec!["shell".into(), "read_file".into()],
            steps: vec![
                "Run tests".into(),
                "Check for uncommitted changes".into(),
                "Build release".into(),
                "Push to remote".into(),
                "Verify deployment".into(),
            ],
            tags: vec!["deploy".into(), "production".into()],
            priority: 9,
            enabled: true,
            ..Skill::new("deploy", "Deploy code to production")
        },
        Skill {
            name: "debug".into(),
            description: "Debug an issue systematically".into(),
            triggers: vec!["debug".into(), "fix bug".into(), "investigate".into()],
            tools_required: vec!["read_file".into(), "shell".into(), "grep".into()],
            steps: vec![
                "Reproduce the issue".into(),
                "Read relevant source code".into(),
                "Check logs and traces".into(),
                "Form hypothesis".into(),
                "Test hypothesis".into(),
                "Implement fix".into(),
            ],
            tags: vec!["debug".into(), "fix".into()],
            priority: 7,
            enabled: true,
            ..Skill::new("debug", "Debug an issue systematically")
        },
    ]
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_new() {
        let s = Skill::new("test", "a test skill");
        assert_eq!(s.name, "test");
        assert!(s.enabled);
    }

    #[test]
    fn test_skill_trigger_match() {
        let s = Skill {
            triggers: vec!["review".into(), "code review".into()],
            ..Skill::new("code-review", "Review code")
        };
        assert!(s.matches_trigger("please review my code"));
        assert!(s.matches_trigger("CODE REVIEW requested"));
        assert!(!s.matches_trigger("deploy the app"));
    }

    #[test]
    fn test_skill_to_prompt() {
        let s = Skill {
            steps: vec!["Step 1".into(), "Step 2".into()],
            examples: vec!["Example 1".into()],
            ..Skill::new("test", "A test skill")
        };
        let prompt = s.to_prompt();
        assert!(prompt.contains("Step 1"));
        assert!(prompt.contains("Example 1"));
    }

    #[test]
    fn test_parse_markdown() {
        let loader = SkillLoader::new(Path::new("/tmp"));
        let md = r#"# my-skill

Description: A useful skill

Triggers: run, execute, do

## Steps

1. Do something
2. Do something else

## Examples

- Example one
- Example two
"#;
        let skill = loader
            .parse_markdown_skill(md, Path::new("/tmp/my-skill.md"))
            .unwrap();
        assert_eq!(skill.name, "my-skill");
        assert_eq!(skill.triggers, vec!["run", "execute", "do"]);
        assert_eq!(skill.steps.len(), 2);
        assert_eq!(skill.examples.len(), 2);
    }

    #[test]
    fn test_builtin_skills() {
        let skills = builtin_skills();
        assert!(skills.len() >= 3);
        assert!(skills.iter().any(|s| s.name == "code-review"));
        assert!(skills.iter().any(|s| s.name == "deploy"));
    }

    #[test]
    fn test_skill_to_markdown() {
        let loader = SkillLoader::new(Path::new("/tmp"));
        let s = Skill {
            triggers: vec!["test".into()],
            tools_required: vec!["shell".into()],
            tags: vec!["test".into()],
            steps: vec!["Step 1".into()],
            ..Skill::new("my-skill", "My skill")
        };
        let md = loader.skill_to_markdown(&s);
        assert!(md.contains("# my-skill"));
        assert!(md.contains("Triggers: test"));
        assert!(md.contains("Step 1"));
    }

    #[test]
    fn test_skill_priority_sort() {
        let mut loader = SkillLoader::new(Path::new("/tmp"));
        loader.skills.insert(
            "low".into(),
            Skill {
                priority: 2,
                triggers: vec!["test".into()],
                ..Skill::new("low", "low priority")
            },
        );
        loader.skills.insert(
            "high".into(),
            Skill {
                priority: 9,
                triggers: vec!["test".into()],
                ..Skill::new("high", "high priority")
            },
        );
        let matches = loader.find_matching("test this");
        assert_eq!(matches[0].name, "high");
        assert_eq!(matches[1].name, "low");
    }
}
