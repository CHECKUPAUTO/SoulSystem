use crate::agents::base::AgentBase;
use crate::errors::CoreError;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A task suggested by the Discovery agent.
#[derive(Debug, Clone, Serialize, Deserialize, garde::Validate)]
pub struct SuggestedTask {
    #[garde(length(min = 1))]
    pub task: String,
    #[garde(length(min = 1))]
    pub url: String,
    #[garde(skip)]
    pub priority: f32,
}

#[derive(Deserialize, garde::Validate)]
struct DiscoveryResponse {
    #[garde(dive)]
    suggestions: Vec<SuggestedTask>,
}

const DISCOVERY_SYSTEM_PROMPT: &str = "You are a discovery-focused AI agent for the AVID system.
Your goal is to analyze web crawl results and identify interesting APIs or logic to clone.
Respond only with valid JSON.";

/// The Discovery agent finds new targets for exploration.
#[derive(Debug, Clone)]
pub struct DiscoveryAgent {
    agent: AgentBase,
}

impl DiscoveryAgent {
    #[must_use]
    pub fn new(agent: AgentBase) -> Self {
        Self { agent }
    }

    /// Discover new tasks from a page's content and links.
    pub async fn discover(
        &self,
        url: &str,
        content: &str,
        links: &[String],
    ) -> Result<Vec<SuggestedTask>, CoreError> {
        let user_prompt = format!(
            "Analyze the content and links from {url} and suggest new cloning tasks.\n\nContent:\n{content}\n\nLinks:\n{links:?}"
        );

        let response: DiscoveryResponse = self
            .agent
            .chat_and_parse(
                DISCOVERY_SYSTEM_PROMPT,
                &user_prompt,
                Duration::from_secs(45),
                2,
            )
            .await?;

        Ok(response.suggestions)
    }
}
