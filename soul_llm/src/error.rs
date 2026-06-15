use std::fmt;

/// Erreurs unifiées pour tous les providers LLM.
#[derive(Debug)]
pub enum LlmError {
    /// Erreur réseau / HTTP
    Network(String),
    /// Clé API invalide ou manquante
    Auth(String),
    /// Rate limiting (429)
    RateLimited { retry_after: Option<u64> },
    /// Budget token dépassé
    BudgetExceeded {
        goal_id: String,
        used: usize,
        budget: usize,
    },
    /// Provider inconnu
    UnknownProvider(String),
    /// Erreur du provider (5xx, modèle introuvable, etc.)
    Provider(String),
    /// Erreur de sérialisation / désérialisation
    Serialization(String),
    /// Le provider ne supporte pas cette opération (ex: embeddings sur Anthropic)
    Unsupported(String),
    /// Timeout
    Timeout(String),
}

impl fmt::Display for LlmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network(e) => write!(f, "network error: {e}"),
            Self::Auth(e) => write!(f, "auth error: {e}"),
            Self::RateLimited { retry_after } => {
                write!(f, "rate limited")?;
                if let Some(s) = retry_after {
                    write!(f, " (retry after {s}s)")?;
                }
                Ok(())
            }
            Self::BudgetExceeded {
                goal_id,
                used,
                budget,
            } => {
                write!(f, "budget exceeded for goal {goal_id}: {used} > {budget}")
            }
            Self::UnknownProvider(p) => write!(f, "unknown provider: {p}"),
            Self::Provider(e) => write!(f, "provider error: {e}"),
            Self::Serialization(e) => write!(f, "serialization error: {e}"),
            Self::Unsupported(e) => write!(f, "unsupported: {e}"),
            Self::Timeout(e) => write!(f, "timeout: {e}"),
        }
    }
}

impl std::error::Error for LlmError {}

impl From<reqwest::Error> for LlmError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Self::Timeout(e.to_string())
        } else {
            Self::Network(e.to_string())
        }
    }
}

impl From<serde_json::Error> for LlmError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization(e.to_string())
    }
}

impl From<String> for LlmError {
    fn from(e: String) -> Self {
        Self::Provider(e)
    }
}

impl From<&str> for LlmError {
    fn from(e: &str) -> Self {
        Self::Provider(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, LlmError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_error_display_network() {
        assert!(LlmError::Network("timeout".into())
            .to_string()
            .contains("network"));
    }

    #[test]
    fn llm_error_display_auth() {
        assert!(LlmError::Auth("bad key".into())
            .to_string()
            .contains("auth"));
    }

    #[test]
    fn llm_error_display_rate_limited() {
        let e = LlmError::RateLimited {
            retry_after: Some(30),
        };
        assert!(e.to_string().contains("retry after 30s"));
    }

    #[test]
    fn llm_error_display_budget_exceeded() {
        let e = LlmError::BudgetExceeded {
            goal_id: "g1".into(),
            used: 200,
            budget: 100,
        };
        assert!(e.to_string().contains("g1"));
    }

    #[test]
    fn llm_error_display_unknown_provider() {
        assert!(LlmError::UnknownProvider("foo".into())
            .to_string()
            .contains("foo"));
    }

    #[test]
    fn llm_error_display_provider() {
        assert!(LlmError::Provider("boom".into())
            .to_string()
            .contains("provider error"));
    }

    #[test]
    fn llm_error_display_serialization() {
        assert!(LlmError::Serialization("bad json".into())
            .to_string()
            .contains("serialization"));
    }

    #[test]
    fn llm_error_display_unsupported() {
        assert!(LlmError::Unsupported("no embeddings".into())
            .to_string()
            .contains("unsupported"));
    }

    #[test]
    fn llm_error_from_string() {
        let e: LlmError = "hello".to_string().into();
        assert!(e.to_string().contains("provider error"));
    }

    #[test]
    fn llm_error_from_str() {
        let e: LlmError = "world".into();
        assert!(e.to_string().contains("provider error"));
    }
}
