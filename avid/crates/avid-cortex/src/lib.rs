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
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate)]
pub struct PaperSummary {
    #[garde(length(min = 1))]
    pub title: String,
    #[garde(length(min = 1))]
    pub abstract_text: String,
    #[garde(skip)]
    pub key_findings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate)]
pub struct Workflow {
    #[garde(skip)]
    pub steps: Vec<String>,
    #[garde(skip)]
    pub entities: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CortexError {
    #[error("agent error: {0}")]
    Agent(#[from] CoreError),
    #[error("PDF error: {0}")]
    Pdf(String),
}

const CORTEX_SYSTEM_PROMPT: &str = "You are a cortex AI agent. Respond in JSON.";

#[derive(Debug, Clone)]
pub struct CortexEngine {
    agent: AgentBase,
}

impl CortexEngine {
    /// Creates a new `CortexEngine`.
    #[must_use]
    pub fn new(agent: AgentBase) -> Self {
        Self { agent }
    }

    /// Reads a paper from the provided PDF bytes.
    ///
    /// # Errors
    /// Returns a `CortexError` if the PDF fails to load or the agent fails to analyze it.
    pub async fn read_paper(&self, pdf_bytes: &[u8]) -> Result<PaperSummary, CortexError> {
        let doc = lopdf::Document::load_mem(pdf_bytes)
            .map_err(|e| CortexError::Pdf(format!("failed to load PDF: {e}")))?;
        let mut text = String::new();
        for page_num in doc.get_pages().keys() {
            if let Ok(page_text) = doc.extract_text(&[*page_num]) {
                text.push_str(&page_text);
                text.push('\n');
            }
        }
        let user_prompt = format!("Analyze paper: {text}");
        let summary: PaperSummary = self
            .agent
            .chat_and_parse(
                CORTEX_SYSTEM_PROMPT,
                &user_prompt,
                Duration::from_secs(60),
                2,
            )
            .await?;
        Ok(summary)
    }

    /// Extracts a workflow from the provided text.
    ///
    /// # Errors
    /// Returns a `CortexError` if the agent fails to extract the workflow.
    pub async fn extract_workflow(&self, text: &str) -> Result<Workflow, CortexError> {
        let user_prompt = format!("Extract workflow: {text}");
        let workflow: Workflow = self
            .agent
            .chat_and_parse(
                CORTEX_SYSTEM_PROMPT,
                &user_prompt,
                Duration::from_secs(30),
                2,
            )
            .await?;
        Ok(workflow)
    }
}
