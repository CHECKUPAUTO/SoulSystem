use std::sync::Arc;

use tracing::error;

use crate::llm::ChatBackend;

/// Common functionality for all agents.
///
/// Holds an `Arc<dyn ChatBackend>` so that any LLM-flavoured backend
/// (raw `LlmClient`, token-tracked wrapper, mock for tests, ...) can
/// be injected uniformly.
#[derive(Clone)]
pub struct AgentBase {
    pub client: Arc<dyn ChatBackend>,
}

impl std::fmt::Debug for AgentBase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentBase")
            .field("client", &self.client)
            .finish()
    }
}

impl AgentBase {
    #[must_use]
    pub fn new(client: Arc<dyn ChatBackend>) -> Self {
        Self { client }
    }

    /// Convenience constructor for the most common case: wrap a
    /// concrete `LlmClient` into `Arc<dyn ChatBackend>`.
    #[must_use]
    pub fn from_llm_client(client: crate::llm::LlmClient) -> Self {
        Self {
            client: Arc::new(client),
        }
    }

    /// Try to parse and validate LLM JSON output, with retries.
    pub async fn chat_and_parse<T>(
        &self,
        system: &str,
        user: &str,
        timeout: std::time::Duration,
        max_retries: u32,
    ) -> Result<T, crate::errors::CoreError>
    where
        T: serde::de::DeserializeOwned + garde::Validate,
        <T as garde::Validate>::Context: Default,
    {
        let mut last_err = String::new();
        for attempt in 0..=max_retries {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }

            let raw = match self.client.chat(system, user, true, timeout).await {
                Ok(r) => r,
                Err(crate::llm::LlmError::Http(e)) if e.is_timeout() => {
                    return Err(crate::errors::CoreError::Timeout);
                }
                Err(e) => return Err(e.into()),
            };

            match serde_json::from_str::<T>(&raw) {
                Ok(parsed) => {
                    if let Err(e) = parsed.validate() {
                        last_err = format!("validation: {e}");
                        error!("agent validation failed (attempt {attempt}): {last_err}");
                        continue;
                    }
                    return Ok(parsed);
                }
                Err(e) => {
                    last_err = format!("parse: {e}");
                    error!("agent JSON parse failed (attempt {attempt}): {last_err}");
                }
            }
        }
        Err(crate::errors::CoreError::Agent(format!(
            "schema invalid after {max_retries} retries: {last_err}"
        )))
    }
}
