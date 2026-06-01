use std::sync::Arc;
use std::time::Duration;

use tracing::{debug, error, info};

use crate::metadata::Capability;
use crate::registry::SkillRegistry;

/// Executes skills by invoking the LLM with skill-specific system prompts.
///
/// Wraps a [`SkillRegistry`](crate::SkillRegistry) and an
/// [`LlmClient`](avid_core::llm::LlmClient) to run skills by name or by capability.
///
/// # Examples
///
/// ```rust,no_run
/// # use std::sync::Arc;
/// # use avid_skills::{SkillRegistry, SkillExecutor};
/// # use avid_core::llm::LlmClient;
/// # use std::path::Path;
/// #
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mut registry = SkillRegistry::new();
/// registry.load_from_dir(Path::new("skills"))?;
///
/// let executor = SkillExecutor::new(registry);
/// let client = LlmClient::new("http://localhost:11434".into(), "llama3:8b".into()).await?;
///
/// let result = executor.execute("code-review", "fn main() {}", &client).await?;
/// println!("{}", result.output);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct SkillExecutor {
    registry: Arc<SkillRegistry>,
}

/// The result of executing a skill.
#[derive(Debug, Clone)]
pub struct SkillResult {
    /// Name of the skill that produced this result
    pub skill_name: String,
    /// Raw LLM output
    pub output: String,
    /// Approximate token count (input + output)
    pub tokens_used: u64,
    /// Wall-clock duration of the LLM call in milliseconds
    pub duration_ms: u64,
    /// Whether the execution completed without errors
    pub success: bool,
}

impl SkillResult {
    /// Create a success result.
    #[must_use]
    pub const fn success(
        skill_name: String,
        output: String,
        tokens_used: u64,
        duration_ms: u64,
    ) -> Self {
        Self {
            skill_name,
            output,
            tokens_used,
            duration_ms,
            success: true,
        }
    }

    /// Create a failure result (empty output, zero tokens).
    #[must_use]
    pub fn failure(skill_name: String, error: &str) -> Self {
        Self {
            skill_name,
            output: error.to_string(),
            tokens_used: 0,
            duration_ms: 0,
            success: false,
        }
    }
}

impl SkillExecutor {
    /// Create a new executor backed by the given registry.
    #[must_use]
    pub fn new(registry: SkillRegistry) -> Self {
        Self {
            registry: Arc::new(registry),
        }
    }

    /// Create a new executor with a pre-Arc'd registry.
    #[must_use]
    pub const fn with_arc(registry: Arc<SkillRegistry>) -> Self {
        Self { registry }
    }

    /// Execute a skill by name.
    ///
    /// Looks up the skill in the registry, then sends `input` as the user
    /// message to the LLM, using the skill's system prompt and configured
    /// timeout/token limits.
    ///
    /// # Errors
    ///
    /// Returns `SkillExecutorError::NotFound` if the skill doesn't exist, or
    /// propagates LLM errors.
    pub async fn execute(
        &self,
        skill_name: &str,
        input: &str,
        llm: &avid_core::llm::LlmClient,
    ) -> Result<SkillResult, SkillExecutorError> {
        self.execute_via(skill_name, input, llm).await
    }

    /// Execute a skill by name via a generic chat backend.
    pub async fn execute_via(
        &self,
        skill_name: &str,
        input: &str,
        llm: &dyn avid_core::llm::ChatBackend,
    ) -> Result<SkillResult, SkillExecutorError> {
        let skill = self
            .registry
            .get(skill_name)
            .ok_or_else(|| SkillExecutorError::NotFound {
                name: skill_name.to_string(),
            })?;

        let effective_model = skill
            .metadata
            .model_preference
            .as_deref()
            .unwrap_or_else(|| llm.model());
        info!(
            "Executing skill '{}' with model '{}'",
            skill_name, effective_model
        );

        let timeout = Duration::from_secs(skill.metadata.timeout_seconds.unwrap_or(120));

        let start = std::time::Instant::now();
        match llm.chat(&skill.system_prompt, input, false, timeout).await {
            Ok(output) => {
                let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
                let tokens = estimate_tokens(&skill.system_prompt, input, &output);
                debug!(
                    "Skill '{}' completed in {}ms, ~{} tokens",
                    skill_name, duration_ms, tokens
                );
                Ok(SkillResult::success(
                    skill.metadata.name.clone(),
                    output,
                    tokens,
                    duration_ms,
                ))
            }
            Err(e) => {
                let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
                error!(
                    "Skill '{}' failed after {}ms: {}",
                    skill_name, duration_ms, e
                );
                Err(SkillExecutorError::Execution {
                    name: skill_name.to_string(),
                    cause: e.to_string(),
                })
            }
        }
    }

    /// Execute a skill by capability, choosing the first matching skill.
    ///
    /// This is a convenience method for routing — it picks the first skill
    /// that provides the requested capability and delegates to
    /// [`execute`](Self::execute). If you need to try multiple skills, use
    /// [`find_by_capability`](SkillRegistry::find_by_capability) and
    /// iterate manually.
    ///
    /// # Errors
    ///
    /// Returns `SkillExecutorError::NotFound` if no skill matches the capability.
    pub async fn execute_by_capability(
        &self,
        cap: &Capability,
        input: &str,
        llm: &avid_core::llm::LlmClient,
    ) -> Result<SkillResult, SkillExecutorError> {
        self.execute_by_capability_via(cap, input, llm).await
    }

    /// Execute a skill by capability via a generic chat backend.
    pub async fn execute_by_capability_via(
        &self,
        cap: &Capability,
        input: &str,
        llm: &dyn avid_core::llm::ChatBackend,
    ) -> Result<SkillResult, SkillExecutorError> {
        let skills = self.registry.find_by_capability(cap);
        if skills.is_empty() {
            return Err(SkillExecutorError::NoCapability {
                capability: cap.to_string(),
            });
        }

        let skill = skills
            .first()
            .ok_or_else(|| SkillExecutorError::NoCapability {
                capability: cap.to_string(),
            })?;
        self.execute_via(&skill.metadata.name, input, llm).await
    }

    /// Expose the underlying registry for inspection.
    #[must_use]
    pub const fn registry(&self) -> &Arc<SkillRegistry> {
        &self.registry
    }
}

/// Errors from skill execution.
#[derive(Debug, thiserror::Error)]
pub enum SkillExecutorError {
    #[error("Skill '{name}' not found in registry")]
    NotFound { name: String },

    #[error("No skill registered for capability '{capability}'")]
    NoCapability { capability: String },

    #[error("LLM execution failed for skill '{name}': {cause}")]
    Execution { name: String, cause: String },

    #[error("LLM error: {0}")]
    Llm(#[from] avid_core::llm::LlmError),
}

// ---------------------------------------------------------------------------
// Approximate token counting
// ---------------------------------------------------------------------------

/// Rough estimate of token count based on character split.
///
/// Uses the rule of thumb: 1 token ≈ 4 characters for English text.
const fn estimate_tokens(system: &str, input: &str, output: &str) -> u64 {
    let chars = system.len() + input.len() + output.len();
    (chars as u64).div_ceil(4)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{Skill, SkillMetadata};
    use std::path::Path;

    fn make_skill(name: &str, caps: Vec<Capability>) -> Skill {
        Skill {
            metadata: SkillMetadata {
                name: name.to_string(),
                version: "1.0.0".into(),
                description: "Test".into(),
                author: "Test".into(),
                capabilities: caps,
                tools_allowed: vec![],
                model_preference: None,
                max_tokens: None,
                timeout_seconds: Some(5),
            },
            system_prompt: "Test system prompt".into(),
            source_path: Path::new("test").join(name).join("SKILL.md"),
        }
    }

    #[test]
    fn test_skill_result_constructors() {
        let ok = SkillResult::success("test".into(), "output".into(), 100, 500);
        assert!(ok.success);
        assert_eq!(ok.tokens_used, 100);

        let fail = SkillResult::failure("test".into(), "bad thing happened");
        assert!(!fail.success);
        assert!(fail.output.contains("bad thing"));
    }

    #[test]
    fn test_estimate_tokens() {
        // 40 chars → ceil(40/4) = 10
        assert_eq!(estimate_tokens("1234", "5678", "9012"), 3); // 12 chars → 3
        assert_eq!(estimate_tokens("", "", "123"), 1); // 3 chars → ceil(3/4)=1
    }

    #[test]
    fn test_executor_not_found() {
        let reg = SkillRegistry::new();
        let executor = SkillExecutor::new(reg);

        // We can't actually execute without a real LLM, but we can check the registry
        assert!(executor.registry().get("nonexistent").is_none());
    }

    #[test]
    fn test_executor_create() {
        let mut reg = SkillRegistry::new();
        reg.register(make_skill("test", vec![Capability::CodeReview]))
            .unwrap();
        let executor = SkillExecutor::new(reg);
        assert!(executor.registry().get("test").is_some());
    }
}
