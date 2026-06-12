#![forbid(unsafe_code)]
#![deny(warnings)]
#![warn(clippy::pedantic, clippy::nursery)]
#![allow(
    clippy::module_name_repetitions,
    clippy::unused_async,
    clippy::missing_const_for_fn
)]

#[allow(warnings)]
pub mod learner;
#[allow(warnings)]
pub mod macros;
#[allow(warnings)]
pub mod player;
#[allow(warnings)]
pub mod recorder;
#[allow(warnings)]
pub mod replay;
#[allow(warnings)]
pub mod validator;

use avid_core::agents::base::AgentBase;
use avid_core::errors::CoreError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct APISpec {
    pub base_url: String,
    pub endpoints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate)]
pub struct CloneOutput {
    #[garde(length(min = 1))]
    pub language: String,
    #[garde(length(min = 1))]
    pub files: HashMap<String, String>,
    #[garde(length(min = 1))]
    pub entrypoint: String,
}

#[derive(Debug, thiserror::Error)]
pub enum MimicError {
    #[error("agent error: {0}")]
    Agent(#[from] CoreError),
}

const MIMIC_SYSTEM_PROMPT: &str = "You are a mimic AI agent. Respond in JSON.";

#[derive(Debug, Clone)]
pub struct MimicEngine {
    agent: AgentBase,
}

impl MimicEngine {
    /// Creates a new `MimicEngine`.
    #[must_use]
    pub fn new(agent: AgentBase) -> Self {
        Self { agent }
    }

    /// Clones an API from the provided spec.
    ///
    /// # Errors
    /// Returns a `MimicError` if the agent fails to clone the API.
    pub async fn clone_api(&self, _spec: &APISpec) -> Result<CloneOutput, MimicError> {
        let user_prompt = "Clone the API spec provided in previous context.".to_string();
        let output: CloneOutput = self
            .agent
            .chat_and_parse(
                MIMIC_SYSTEM_PROMPT,
                &user_prompt,
                Duration::from_secs(60),
                2,
            )
            .await?;
        Ok(output)
    }

    /// Clones logic from the provided description.
    ///
    /// # Errors
    /// Returns a `MimicError` if the agent fails to clone the logic.
    pub async fn clone_logic(&self, description: &str) -> Result<CloneOutput, MimicError> {
        let user_prompt = format!("Clone logic: {description}");
        let output: CloneOutput = self
            .agent
            .chat_and_parse(
                MIMIC_SYSTEM_PROMPT,
                &user_prompt,
                Duration::from_secs(60),
                2,
            )
            .await?;
        Ok(output)
    }
}
