//! Types and errors for TensorRT-LLM inference.

use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TrtError {
    #[error("Failed to initialize TensorRT-LLM runtime: {0}")]
    Init(String),
    #[error("Failed to load engine from path {path}: {details}")]
    Load { path: String, details: String },
    #[error("Generation error: {0}")]
    Generate(String),
    #[error("Out of Memory (VRAM): {0}")]
    OutOfMemory(String),
    #[error("TensorRT-LLM feature is disabled. Fallback to Ollama required.")]
    FeatureDisabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrtConfig {
    pub max_batch_size: u32,
    pub max_input_len: u32,
    pub max_output_len: u32,
    pub enable_kv_cache_reuse: bool,
}

impl Default for TrtConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 4,
            max_input_len: 4096,
            max_output_len: 2048,
            enable_kv_cache_reuse: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationConfig {
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: u32,
    pub repetition_penalty: f32,
    pub max_new_tokens: u32,
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            top_p: 0.9,
            top_k: 40,
            repetition_penalty: 1.1,
            max_new_tokens: 2048,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GenerationResult {
    pub text: String,
    pub token_count: usize,
    pub latency: Duration,
    pub ttft: Duration,
}

#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub text: String,
    pub is_last: bool,
    pub latency: Duration,
}

/// A batch item with priority routing.
/// Think = high priority (GPU), Dream = low priority (CPU NUMA).
#[derive(Debug, Clone)]
pub struct BatchItem {
    pub request_id: String,
    pub prompt: String,
    pub config: GenerationConfig,
    pub priority: BatchPriority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BatchPriority {
    Think = 0, // GPU, immediate
    Embed = 1, // GPU, batched
    Dream = 2, // CPU NUMA, background
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let config = TrtConfig::default();
        assert_eq!(config.max_batch_size, 4);
        assert_eq!(config.max_input_len, 4096);
        assert!(config.enable_kv_cache_reuse);
    }

    #[test]
    fn default_generation_config() {
        let gen = GenerationConfig::default();
        assert!((gen.temperature - 0.7).abs() < 0.01);
        assert_eq!(gen.max_new_tokens, 2048);
    }

    #[test]
    fn priority_ordering() {
        assert!(BatchPriority::Think < BatchPriority::Embed);
        assert!(BatchPriority::Embed < BatchPriority::Dream);
    }

    #[test]
    fn trt_error_display() {
        let err = TrtError::FeatureDisabled;
        assert!(err.to_string().contains("Fallback"));
    }

    #[test]
    fn batch_item_creation() {
        let item = BatchItem {
            request_id: "test-001".to_string(),
            prompt: "Hello".to_string(),
            config: GenerationConfig::default(),
            priority: BatchPriority::Think,
        };
        assert_eq!(item.priority, BatchPriority::Think);
    }
}
