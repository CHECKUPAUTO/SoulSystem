use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::agents::base::AgentBase;
use crate::errors::CoreError;
use crate::memory::MemoryStore;
use avid_anticlone::Submission;

const SYSTEM_PROMPT: &str = "\
You are a senior software architect. Your role is to merge multiple Python code submissions from sub-tasks into a single, cohesive project.

Given a list of sub-tasks and their corresponding Python files, produce a final JSON response with:
- modules: array of {path: relative filename ending in .py, purpose: short description}
- files: object mapping path to complete Python source code
- entrypoint: the filename (must be a key in files) to execute

Ensure that all modules work together correctly, dependencies are resolved, and the entrypoint provides the final combined functionality.
All code must be syntactically valid Python.

Output only valid JSON. No additional text.";

#[derive(serde::Deserialize, garde::Validate)]
struct RawSubmission {
    #[garde(dive)]
    #[allow(dead_code)]
    modules: Vec<crate::models::Module>,
    #[garde(length(min = 1))]
    files: HashMap<String, String>,
    #[garde(length(min = 1))]
    entrypoint: String,
}

/// The SubtaskAggregator agent merges results from multiple sub-tasks.
pub struct SubtaskAggregator {
    base: AgentBase,
    memory: Arc<MemoryStore>,
}

impl SubtaskAggregator {
    #[must_use]
    pub fn new(base: AgentBase, memory: Arc<MemoryStore>) -> Self {
        Self { base, memory }
    }

    /// Aggregate results from sub-tasks into a final submission.
    pub async fn aggregate(&self, subtask_ids: &[String]) -> Result<Submission, CoreError> {
        let mut subtask_results = Vec::new();

        for id in subtask_ids {
            if let Some(result) = self.memory.get_result(id).await? {
                subtask_results.push(result);
            } else {
                return Err(CoreError::Agent(format!("missing result for subtask {id}")));
            }
        }

        let subtask_context: String = subtask_results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let task = r["task"].as_str().unwrap_or("");
                let submission = r.get("submission").cloned().unwrap_or_default();
                format!(
                    "Subtask {}:\nDescription: {}\nSubmission: {}\n",
                    i + 1,
                    task,
                    submission
                )
            })
            .collect::<Vec<_>>()
            .join("\n---\n");

        let user_prompt = format!("Sub-tasks to aggregate:\n{subtask_context}\n\nPlease produce the final merged project.");

        let raw: RawSubmission = self
            .base
            .chat_and_parse(SYSTEM_PROMPT, &user_prompt, Duration::from_secs(90), 2)
            .await?;

        Ok(Submission {
            files: raw.files,
            entrypoint: raw.entrypoint,
        })
    }
}
