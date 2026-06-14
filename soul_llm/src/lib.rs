//! # soul_llm — Client LLM multi-provider avec streaming
//!
//! Supporte Ollama, OpenAI et Anthropic nativement.
//! Le workspace utilise `LlmClient` — un wrapper unifié qui gère le provider
//! et le budget automatiquement.
//!
//! ## Usage
//!
//! ```ignore
//! use soul_llm::{LlmClient, LlmConfig, ProviderKind};
//!
//! let config = LlmConfig {
//!     provider: ProviderKind::OpenAI,
//!     base_url: "https://api.openai.com".into(),
//!     model: "gpt-4o".into(),
//!     auth_token: Some("sk-...".into()),
//!     ..Default::default()
//! };
//!
//! let client = LlmClient::new(config).unwrap();
//! let result = client.generate("Hello").await.unwrap();
//! println!("{}", result.text);
//! ```

pub mod budget;
pub mod chat;
pub mod client;
pub mod error;
pub mod factory;
pub mod provider;
pub mod providers;
pub mod types;

// Re-exports principaux
pub use budget::LlmBudget;
pub use client::LlmClient;
pub use error::{LlmError, Result as LlmResult};
pub use factory::{create_provider, create_provider_by_name};
pub use provider::LlmProvider;
pub use types::{
    EmbeddingResult, GenerateRequest, GenerateResult, LlmConfig, ModelInfo, ProviderKind,
    StreamChunk, TokenUsage,
};

// Compat chat + tool-calling layer (used by soul_agent_core's ReAct loop).
pub use chat::{
    build_tool_schemas, ChatMessage, ChatResponse, ChatResponseMessage, ChatSession, FunctionCall,
    FunctionSchema, GenerateResponse, OllamaClient, Role, ToolCall, ToolSchema,
};

// backward compat
pub use types::ProviderKind as LlmProviderKind;
pub use types::TokenUsage as LlmUsage;
