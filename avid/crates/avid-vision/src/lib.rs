#![forbid(unsafe_code)]
#![deny(warnings)]
#![warn(clippy::pedantic, clippy::nursery)]
#![allow(
    clippy::module_name_repetitions,
    clippy::unused_async,
    clippy::missing_const_for_fn
)]

use avid_core::agents::base::AgentBase;
use avid_core::errors::CoreError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate)]
pub struct UIComponent {
    #[garde(length(min = 1))]
    pub tag: String,
    #[garde(skip)]
    pub attributes: HashMap<String, String>,
    #[garde(skip)]
    pub children_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate)]
pub struct Pattern {
    #[garde(length(min = 1))]
    pub name: String,
    #[garde(range(min = 0.0, max = 1.0))]
    pub confidence: f32,
}

#[derive(Deserialize, garde::Validate)]
struct UIAnalysisResponse {
    #[garde(dive)]
    components: Vec<UIComponent>,
}

#[derive(Deserialize, garde::Validate)]
struct PatternDetectionResponse {
    #[garde(dive)]
    patterns: Vec<Pattern>,
}

#[derive(Debug, thiserror::Error)]
pub enum VisionError {
    #[error("agent error: {0}")]
    Agent(#[from] CoreError),
}

const VISION_SYSTEM_PROMPT: &str = "You are a vision AI agent. Respond in JSON.";

#[derive(Debug, Clone)]
pub struct VisionEngine {
    agent: AgentBase,
}

impl VisionEngine {
    /// Creates a new `VisionEngine`.
    #[must_use]
    pub fn new(agent: AgentBase) -> Self {
        Self { agent }
    }

    /// Analyzes the UI of the provided HTML.
    ///
    /// # Errors
    /// Returns a `VisionError` if the agent fails to analyze the UI.
    pub async fn analyze_ui(&self, html: &str) -> Result<Vec<UIComponent>, VisionError> {
        let user_prompt = format!("Analyze HTML: {html}");
        let response: UIAnalysisResponse = self
            .agent
            .chat_and_parse(
                VISION_SYSTEM_PROMPT,
                &user_prompt,
                Duration::from_secs(30),
                2,
            )
            .await?;
        Ok(response.components)
    }

    /// Detects patterns in the provided UI structure.
    ///
    /// # Errors
    /// Returns a `VisionError` if the agent fails to detect patterns.
    pub async fn detect_patterns(&self, structure: &str) -> Result<Vec<Pattern>, VisionError> {
        let user_prompt = format!("Detect patterns: {structure}");
        let response: PatternDetectionResponse = self
            .agent
            .chat_and_parse(
                VISION_SYSTEM_PROMPT,
                &user_prompt,
                Duration::from_secs(30),
                2,
            )
            .await?;
        Ok(response.patterns)
    }
}
