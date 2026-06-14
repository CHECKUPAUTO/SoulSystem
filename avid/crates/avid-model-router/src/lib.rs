//! `avid-model-router` — Intelligent model routing for the AVID system.
//!
//! This crate provides capability-based model selection, a model registry, and
//! fallback chain traversal. It integrates with `avid-core` by providing a
//! utility to construct an [`avid_core::llm::LlmClient`] from a [`ModelProfile`].
//!
//! # Modules
//!
//! - `registry` — the [`ModelRegistry`] for storing and querying model profiles.
//! - `router` — the [`ModelRouter`] for selecting the best model for a task.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use avid_model_router::{ModelRouter, Capability};
//!
//! let router = ModelRouter::with_defaults();
//! let selection = router.select_for_task(
//!     "generate Rust documentation",
//!     &[Capability::CodeGeneration],
//! );
//! eprintln!("Using model: {} because: {}", selection.profile.name, selection.reason);
//! ```

#![forbid(unsafe_code)]
#![warn(clippy::pedantic, clippy::nursery)]
#![deny(clippy::todo, clippy::unimplemented)]
#![allow(
    clippy::module_name_repetitions,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::doc_markdown,
    clippy::must_use_candidate,
    clippy::missing_const_for_fn,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::unnecessary_cast
)]

use serde::{Deserialize, Serialize};

pub mod learned;
pub mod registry;
pub mod router;

pub use learned::{
    CostAwareRouter, DifficultyModel, QueryFeatures, RouterParams, RoutingDecision, RoutingMetrics,
};
pub use registry::{ModelRegistry, RegistryError};
pub use router::{ModelRouter, RouterError};

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// A capability that a model can have.
///
/// Used to match tasks to the appropriate model.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Capability {
    /// Generating, completing, or refactoring source code.
    CodeGeneration,
    /// Reviewing code for correctness, style, security, or performance.
    CodeReview,
    /// Planning, architecture design, or breaking down large tasks.
    Planning,
    /// Deep analysis, debugging, or root-cause investigation.
    Analysis,
    /// Summarizing text, documents, or conversation threads.
    Summarization,
    /// Creative writing, brainstorming, or non-technical generation.
    Creative,
    /// Quick, low-latency chat responses.
    FastChat,
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CodeGeneration => f.write_str("code-generation"),
            Self::CodeReview => f.write_str("code-review"),
            Self::Planning => f.write_str("planning"),
            Self::Analysis => f.write_str("analysis"),
            Self::Summarization => f.write_str("summarization"),
            Self::Creative => f.write_str("creative"),
            Self::FastChat => f.write_str("fast-chat"),
        }
    }
}

impl std::str::FromStr for Capability {
    type Err = String;

    /// Parse a capability from a kebab-case, snake_case, or short alias form
    /// (e.g. `code-generation`, `code_gen`, `codegen`, `chat`).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let norm = s.trim().to_lowercase().replace(['_', ' '], "-");
        let cap = match norm.as_str() {
            "code-generation" | "code-gen" | "codegen" | "code" | "gen" => Self::CodeGeneration,
            "code-review" | "review" => Self::CodeReview,
            "planning" | "plan" => Self::Planning,
            "analysis" | "analyze" | "analyse" => Self::Analysis,
            "summarization" | "summary" | "summarize" | "summ" => Self::Summarization,
            "creative" | "create" => Self::Creative,
            "fast-chat" | "fastchat" | "chat" | "fast" => Self::FastChat,
            other => return Err(format!("unknown capability: '{other}'")),
        };
        Ok(cap)
    }
}

/// Profile describing a model — its name, provider, capabilities, and cost.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfile {
    /// Unique name for this model (e.g. `"deepseek-v4-pro:cloud"`).
    pub name: String,
    /// Provider identifier (e.g. `"ollama"`, `"openai"`, `"anthropic"`).
    pub provider: String,
    /// Base URL for the provider's API endpoint.
    pub base_url: String,
    /// List of capabilities this model supports.
    pub capabilities: Vec<Capability>,
    /// Maximum context window in tokens.
    pub max_tokens: u64,
    /// Estimated cost per 1 000 tokens in USD.
    /// Set to `0.0` for locally-hosted / free models.
    pub cost_per_1k_tokens: f64,
    /// Priority for selection (0–255, lower = preferred).
    pub priority: u8,
    /// Average API round-trip latency in milliseconds.
    pub avg_latency_ms: u64,
}

/// The result of a model selection — the chosen profile plus a human-readable
/// explanation of *why* it was chosen.
#[derive(Debug, Clone)]
pub struct ModelSelection {
    /// The selected model profile.
    pub profile: ModelProfile,
    /// Explanation of the selection decision.
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Integration with avid-core
// ---------------------------------------------------------------------------

/// Create an `avid_core::llm::LlmClient` from a [`ModelProfile`].
///
/// This is a convenience function that calls `LlmClient::new` with the
/// profile's `base_url` and `name`. For Ollama, these correspond to the
/// server URL and model tag respectively.
///
/// # Errors
/// Returns `avid_core::llm::LlmError` if the LLM server is unreachable.
///
/// # Examples
///
/// ```rust,no_run
/// # #[cfg(feature = "llm-client")] {
/// use avid_model_router::{ModelProfile, Capability, create_llm_client};
///
/// let profile = ModelProfile {
///     name: "qwen3:4b".to_string(),
///     provider: "ollama".to_string(),
///     base_url: "http://localhost:11434".to_string(),
///     capabilities: vec![Capability::CodeGeneration],
///     max_tokens: 8192,
///     cost_per_1k_tokens: 0.0,
///     priority: 5,
///     avg_latency_ms: 300,
/// };
/// # async {
/// # let client = create_llm_client(&profile).await.unwrap();
/// # };
/// # }
/// ```
#[cfg(feature = "llm-client")]
pub async fn create_llm_client(
    profile: &ModelProfile,
) -> Result<avid_core::llm::LlmClient, avid_core::llm::LlmError> {
    avid_core::llm::LlmClient::new(profile.base_url.clone(), profile.name.clone()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_serde_roundtrip() {
        let cap = Capability::CodeReview;
        let json = serde_json::to_string(&cap).unwrap();
        let back: Capability = serde_json::from_str(&json).unwrap();
        assert_eq!(cap, back);
    }

    #[test]
    fn capability_display() {
        assert_eq!(Capability::CodeGeneration.to_string(), "code-generation");
        assert_eq!(Capability::FastChat.to_string(), "fast-chat");
        assert_eq!(Capability::Creative.to_string(), "creative");
    }

    #[test]
    fn capability_from_str_accepts_aliases() {
        use std::str::FromStr;
        assert_eq!(
            Capability::from_str("analysis").unwrap(),
            Capability::Analysis
        );
        assert_eq!(
            Capability::from_str("analyze").unwrap(),
            Capability::Analysis
        );
        assert_eq!(
            Capability::from_str("code_gen").unwrap(),
            Capability::CodeGeneration
        );
        assert_eq!(
            Capability::from_str("CodeGen").unwrap(),
            Capability::CodeGeneration
        );
        assert_eq!(Capability::from_str("chat").unwrap(), Capability::FastChat);
        // Display round-trips through FromStr.
        for cap in [
            Capability::CodeGeneration,
            Capability::CodeReview,
            Capability::Planning,
            Capability::Analysis,
            Capability::Summarization,
            Capability::Creative,
            Capability::FastChat,
        ] {
            assert_eq!(Capability::from_str(&cap.to_string()).unwrap(), cap);
        }
        assert!(Capability::from_str("nonsense").is_err());
    }

    #[test]
    fn profile_deserialize_from_json() {
        let json = r#"{
            "name": "test-model",
            "provider": "ollama",
            "base_url": "http://localhost:11434",
            "capabilities": ["fastChat", "codeGeneration"],
            "max_tokens": 8192,
            "cost_per_1k_tokens": 0.0,
            "priority": 5,
            "avg_latency_ms": 300
        }"#;
        let profile: ModelProfile = serde_json::from_str(json).unwrap();
        assert_eq!(profile.name, "test-model");
        assert_eq!(profile.capabilities.len(), 2);
        assert_eq!(profile.capabilities[0], Capability::FastChat);
        assert_eq!(profile.provider, "ollama");
    }
}
