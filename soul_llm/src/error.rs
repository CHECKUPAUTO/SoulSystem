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
    /// The provider answered 5xx — overloaded, or a transient upstream fault.
    ///
    /// Distinct from [`Self::Provider`], which is the catch-all for errors a
    /// repeat of the identical request cannot fix (unknown model, malformed
    /// request). Retry classification needs the two separated, and before
    /// MED-001 the providers never inspected the status at all, so a 503 became
    /// a [`Self::Serialization`] error when the HTML error page failed to parse
    /// as JSON.
    ServiceUnavailable {
        status: u16,
        retry_after: Option<u64>,
    },
    /// A retryable error survived the whole retry budget.
    ///
    /// Carried as a distinct variant so an outer layer can tell "this failed
    /// once, transiently" from "the provider layer already backed off and
    /// retried `attempts` times and it is still failing". The latter should not
    /// be met with an immediate strategy-level replan-and-retry, which is the
    /// uncoordinated behaviour MED-009 describes.
    RetriesExhausted { attempts: u32, last: Box<LlmError> },
    /// This process's shared retry allowance for `provider` is spent
    /// (MED-009-C).
    ///
    /// Deliberately distinct from [`Self::RetriesExhausted`]. That one says
    /// *this request* tried its configured number of times; this one says the
    /// process as a whole is already retrying against this provider as fast as
    /// it is permitted to, so the request was not retried at all.
    ///
    /// Conflating them would report a global rate limit as one unlucky
    /// request, and hide the fact an operator actually needs: the provider is
    /// in trouble and every run is queueing behind it.
    ProviderBudgetExhausted {
        provider: String,
        last: Box<LlmError>,
    },
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
            Self::ServiceUnavailable {
                status,
                retry_after,
            } => {
                write!(f, "service unavailable (HTTP {status})")?;
                if let Some(s) = retry_after {
                    write!(f, " (retry after {s}s)")?;
                }
                Ok(())
            }
            Self::RetriesExhausted { attempts, last } => {
                write!(f, "giving up after {attempts} attempts: {last}")
            }
            Self::ProviderBudgetExhausted { provider, last } => write!(
                f,
                "shared retry budget for provider {provider} is spent; not \
                 retried: {last}"
            ),
        }
    }
}

impl LlmError {
    /// Whether repeating the *identical* request could plausibly succeed.
    ///
    /// This is the single classification both the provider retry loop
    /// ([`crate::retry::RetryPolicy`]) and any outer replan/abort decision
    /// should consult, so the two layers cannot disagree about the same error
    /// (MED-009).
    ///
    /// `Serialization` is deliberately **not** retryable: now that providers
    /// check the HTTP status first, a deserialisation failure means the body
    /// really was malformed, and a deterministic parse bug does not fix itself
    /// on the second try. `RetriesExhausted` is not retryable either — that is
    /// the whole point of the variant.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Network(_)
            | Self::Timeout(_)
            | Self::RateLimited { .. }
            | Self::ServiceUnavailable { .. } => true,

            Self::Auth(_)
            | Self::BudgetExceeded { .. }
            | Self::UnknownProvider(_)
            | Self::Provider(_)
            | Self::Serialization(_)
            | Self::Unsupported(_)
            | Self::RetriesExhausted { .. }
            // Retrying is exactly what is rate-limited here, so treating this
            // as retryable would defeat the ceiling it reports.
            | Self::ProviderBudgetExhausted { .. } => false,
        }
    }

    /// The server's own instruction on when to retry, if it sent one.
    pub fn retry_after(&self) -> Option<std::time::Duration> {
        match self {
            Self::RateLimited {
                retry_after: Some(s),
            }
            | Self::ServiceUnavailable {
                retry_after: Some(s),
                ..
            } => Some(std::time::Duration::from_secs(*s)),
            _ => None,
        }
    }

    /// Map an HTTP status the provider considered unsuccessful onto an error.
    ///
    /// One place rather than three, so the three providers cannot drift on
    /// which status is transient.
    pub fn from_http_status(status: u16, retry_after: Option<u64>, body: &str) -> Self {
        // Bodies can carry an upstream request id or an echoed prompt; keep
        // enough to debug without letting a whole response into the log line.
        let detail: String = body.chars().take(512).collect();
        match status {
            401 | 403 => Self::Auth(format!("HTTP {status}: {detail}")),
            429 => Self::RateLimited { retry_after },
            s if (500..600).contains(&s) => Self::ServiceUnavailable {
                status: s,
                retry_after,
            },
            s => Self::Provider(format!("HTTP {s}: {detail}")),
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
