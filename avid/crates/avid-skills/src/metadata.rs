use std::path::Path;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A capability tag that classifies what a skill can do.
///
/// Used for routing tasks to the appropriate skills via capability-based lookup.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    /// General code analysis and review
    #[serde(rename = "CodeReview")]
    CodeReview,
    /// Code generation from specifications
    #[serde(rename = "CodeGenerate")]
    CodeGenerate,
    /// Task decomposition and planning
    #[serde(rename = "Planning")]
    Planning,
    /// Document and data analysis
    #[serde(rename = "Analysis")]
    Analysis,
    /// General-purpose problem solving
    #[serde(rename = "ProblemSolving")]
    ProblemSolving,
    /// Refactoring and code improvement
    #[serde(rename = "Refactoring")]
    Refactoring,
    /// Testing and test generation
    #[serde(rename = "Testing")]
    Testing,
    /// Documentation generation
    #[serde(rename = "Documentation")]
    Documentation,
    /// Debugging and error diagnosis
    #[serde(rename = "Debugging")]
    Debugging,
    /// Security audit and vulnerability scanning
    #[serde(rename = "SecurityAudit")]
    SecurityAudit,
    /// Other capability not covered by the standard set
    #[serde(rename = "Other")]
    Other(String),
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CodeReview => write!(f, "CodeReview"),
            Self::CodeGenerate => write!(f, "CodeGenerate"),
            Self::Planning => write!(f, "Planning"),
            Self::Analysis => write!(f, "Analysis"),
            Self::ProblemSolving => write!(f, "ProblemSolving"),
            Self::Refactoring => write!(f, "Refactoring"),
            Self::Testing => write!(f, "Testing"),
            Self::Documentation => write!(f, "Documentation"),
            Self::Debugging => write!(f, "Debugging"),
            Self::SecurityAudit => write!(f, "SecurityAudit"),
            Self::Other(name) => write!(f, "Other({name})"),
        }
    }
}

/// Parsed YAML frontmatter from a `SKILL.md` file.
///
/// Contains all the metadata fields extracted from the frontmatter block,
/// including capabilities, model preferences, and execution limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    /// Unique name of the skill (used for lookup)
    pub name: String,
    /// Semantic version
    pub version: String,
    /// Human-readable description of what the skill does
    pub description: String,
    /// Author or maintainer
    pub author: String,
    /// Capability tags for routing
    pub capabilities: Vec<Capability>,
    /// Tools the skill is allowed to invoke
    #[serde(default)]
    pub tools_allowed: Vec<String>,
    /// Preferred model for this skill (e.g. "deepseek-v4-pro:cloud")
    #[serde(default)]
    pub model_preference: Option<String>,
    /// Maximum tokens to allocate for output
    #[serde(default)]
    pub max_tokens: Option<u64>,
    /// Timeout in seconds for skill execution
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

/// A fully loaded skill, ready for execution.
///
/// Combines the parsed metadata with the raw system prompt (the markdown
/// body after the frontmatter) and the original file path.
#[derive(Debug, Clone)]
pub struct Skill {
    /// Parsed frontmatter metadata
    pub metadata: SkillMetadata,
    /// The markdown body after the YAML frontmatter, used as the system prompt
    pub system_prompt: String,
    /// Absolute path to the source SKILL.md file
    pub source_path: PathBuf,
}

/// Parse a `SKILL.md` file into a [`Skill`].
///
/// Extracts YAML frontmatter from between `---` delimiters and treats
/// the remainder as the system prompt.
///
/// # Errors
///
/// Returns an error if:
/// - The file cannot be read
/// - No frontmatter delimiters are found
/// - The YAML frontmatter fails to parse as [`SkillMetadata`]
pub fn parse_skill_file(path: &Path) -> Result<Skill, SkillParseError> {
    let content = std::fs::read_to_string(path).map_err(|e| SkillParseError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;

    let (frontmatter, body) = extract_frontmatter(&content, path)?;

    let metadata: SkillMetadata =
        serde_yaml::from_str(&frontmatter).map_err(|e| SkillParseError::YamlParse {
            path: path.to_path_buf(),
            source: e,
        })?;

    if metadata.name.is_empty() {
        return Err(SkillParseError::MissingField {
            path: path.to_path_buf(),
            field: "name",
        });
    }

    let system_prompt: String = body
        .lines()
        .skip_while(|l| l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    Ok(Skill {
        metadata,
        system_prompt,
        source_path: path.to_path_buf(),
    })
}

/// Extract YAML frontmatter and body from markdown content.
fn extract_frontmatter(content: &str, path: &Path) -> Result<(String, String), SkillParseError> {
    let mut lines = content.lines().peekable();

    // Skip leading blank lines
    while let Some(line) = lines.peek() {
        if line.trim().is_empty() {
            lines.next();
        } else {
            break;
        }
    }

    let first = lines.next().unwrap_or_default();
    if first.trim() != "---" {
        return Err(SkillParseError::NoFrontmatter {
            path: path.to_path_buf(),
        });
    }

    let mut yaml_lines: Vec<&str> = Vec::new();
    let mut found_closing = false;

    for line in lines.by_ref() {
        if line.trim() == "---" {
            found_closing = true;
            break;
        }
        yaml_lines.push(line);
    }

    if !found_closing {
        return Err(SkillParseError::UnclosedFrontmatter {
            path: path.to_path_buf(),
        });
    }

    let frontmatter = yaml_lines.join("\n");
    let body: String = lines.collect::<Vec<_>>().join("\n");

    Ok((frontmatter, body))
}

/// Errors that can occur when parsing a `SKILL.md` file.
#[derive(Debug, thiserror::Error)]
pub enum SkillParseError {
    #[error("IO error reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("No YAML frontmatter found in {path}")]
    NoFrontmatter { path: PathBuf },

    #[error("Unclosed YAML frontmatter in {path}")]
    UnclosedFrontmatter { path: PathBuf },

    #[error("YAML parse error in {path}: {source}")]
    YamlParse {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },

    #[error("Missing required field '{field}' in {path}")]
    MissingField { path: PathBuf, field: &'static str },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_display() {
        assert_eq!(Capability::CodeReview.to_string(), "CodeReview");
        assert_eq!(Capability::Planning.to_string(), "Planning");
        assert_eq!(
            Capability::Other("CustomSkill".into()).to_string(),
            "Other(CustomSkill)"
        );
    }

    #[test]
    fn test_capability_serde_roundtrip() {
        let caps = vec![
            Capability::CodeReview,
            Capability::Analysis,
            Capability::Other("Custom".into()),
        ];
        let json = serde_json::to_string(&caps).unwrap();
        let back: Vec<Capability> = serde_json::from_str(&json).unwrap();
        assert_eq!(caps, back);
    }

    #[test]
    fn test_parse_valid_skill() {
        let tmp = tempfile::TempDir::with_prefix("avid-skills-test").unwrap();
        let path = tmp.path().join("SKILL.md");
        std::fs::write(
            &path,
            r#"---
name: test-skill
version: "1.0.0"
description: "A test skill"
author: "AVID"
capabilities:
  - CodeReview
  - Analysis
---

# Test Skill

You are a tester. Do the thing.
"#,
        )
        .unwrap();

        let skill = parse_skill_file(&path).unwrap();
        assert_eq!(skill.metadata.name, "test-skill");
        assert_eq!(skill.metadata.version, "1.0.0");
        assert_eq!(skill.metadata.capabilities.len(), 2);
        assert!(skill.system_prompt.contains("You are a tester"));
    }

    #[test]
    fn test_parse_missing_frontmatter() {
        let tmp = tempfile::TempDir::with_prefix("avid-skills-test").unwrap();
        let path = tmp.path().join("SKILL.md");
        std::fs::write(&path, "# No frontmatter here").unwrap();

        let err = parse_skill_file(&path).unwrap_err();
        assert!(matches!(err, SkillParseError::NoFrontmatter { .. }));
    }

    #[test]
    fn test_parse_missing_name() {
        let tmp = tempfile::TempDir::with_prefix("avid-skills-test").unwrap();
        let path = tmp.path().join("SKILL.md");
        std::fs::write(
            &path,
            r#"---
version: "1.0"
description: "test"
author: "me"
capabilities: []
---
Body
"#,
        )
        .unwrap();

        let err = parse_skill_file(&path).unwrap_err();
        assert!(matches!(err, SkillParseError::YamlParse { .. }));
    }

    #[test]
    fn test_skill_metadata_defaults() {
        let yaml = r#"
name: default-test
version: "1.0.0"
description: "Testing defaults"
author: "AVID"
capabilities: []
"#;
        let meta: SkillMetadata = serde_yaml::from_str(yaml).unwrap();
        assert!(meta.tools_allowed.is_empty());
        assert!(meta.model_preference.is_none());
        assert!(meta.max_tokens.is_none());
        assert!(meta.timeout_seconds.is_none());
    }
}
