//! TRT Engine — loads models, creates sessions, generates text.
//!
//! Stub mode (default): returns `TrtError::FeatureDisabled` on generate calls.
//! TRT mode (feature = "trt-llm"): full FFI via cxx bridge.

use crate::types::{
    BatchPriority, GenerationConfig, GenerationResult, StreamChunk, TrtConfig, TrtError,
};
use std::path::Path;

/// TensorRT-LLM inference engine.
///
/// Thread-safe (Send + Sync). In stub mode, it's a placeholder that signals
/// the orchestrator to fall back to Ollama HTTP.
pub struct TrtEngine {
    config: TrtConfig,
    model_path: String,
    loaded: bool,
    // In TRT mode, this holds the opaque C++ runtime pointer:
    // #[cfg(feature = "trt-llm")]
    // runtime_ptr: cxx::UniquePtr<ffi::TllmRuntimeWrapper>,
}

// Safety: TrtEngine contains no thread-unsafe interior mutability.
// In TRT mode, the C++ runtime is guarded by internal mutexes.
unsafe impl Send for TrtEngine {}
unsafe impl Sync for TrtEngine {}

impl TrtEngine {
    /// Initialize the engine and load model weights into VRAM.
    ///
    /// In stub mode, this succeeds but marks the engine as not functional.
    /// The orchestrator should check `is_available()` before calling generate.
    pub async fn from_onnx_or_engine<P: AsRef<Path>>(
        path: P,
        config: TrtConfig,
    ) -> Result<Self, TrtError> {
        let model_path = path.as_ref().to_string_lossy().to_string();

        #[cfg(not(feature = "trt-llm"))]
        {
            tracing::warn!(
                model_path = %model_path,
                "soullink-trt compiled in STUB mode — TRT-LLM disabled, will fallback to Ollama"
            );
            Ok(Self {
                config,
                model_path,
                loaded: false,
            })
        }

        #[cfg(feature = "trt-llm")]
        {
            tracing::info!(model_path = %model_path, "Initializing TensorRT-LLM engine");
            let path_clone = model_path.clone();
            let max_batch = config.max_batch_size;

            let _runtime_ptr = tokio::task::spawn_blocking(move || {
                crate::ffi::load_engine(&path_clone, max_batch)
            })
            .await
            .map_err(|e| TrtError::Init(format!("spawn_blocking error: {e}")))??;

            Ok(Self {
                config,
                model_path,
                loaded: true,
            })
        }
    }

    /// Check if TRT-LLM is available (stub mode always returns false).
    pub fn is_available(&self) -> bool {
        #[cfg(feature = "trt-llm")]
        {
            self.loaded
        }

        #[cfg(not(feature = "trt-llm"))]
        {
            false
        }
    }

    /// Get the model path.
    pub fn model_path(&self) -> &str {
        &self.model_path
    }

    /// Get the config.
    pub fn config(&self) -> &TrtConfig {
        &self.config
    }

    /// Synchronous generation — blocks until all tokens are produced.
    ///
    /// Returns `TrtError::FeatureDisabled` in stub mode.
    pub async fn generate(
        &self,
        prompt: &str,
        gen_config: GenerationConfig,
    ) -> Result<GenerationResult, TrtError> {
        #[cfg(not(feature = "trt-llm"))]
        {
            let _ = (prompt, gen_config);
            Err(TrtError::FeatureDisabled)
        }

        #[cfg(feature = "trt-llm")]
        {
            if !self.loaded {
                return Err(TrtError::Init("Engine not loaded".into()));
            }
            // NOTE: Create session, invoke C++ generate, collect tokens
            let _ = (prompt, gen_config);
            Err(TrtError::Init("TRT generation FFI not implemented".into()))
        }
    }

    /// Streaming generation — yields tokens as they're produced.
    ///
    /// Returns `TrtError::FeatureDisabled` in stub mode.
    pub async fn generate_stream(
        &self,
        prompt: &str,
        gen_config: GenerationConfig,
    ) -> Result<Vec<StreamChunk>, TrtError> {
        #[cfg(not(feature = "trt-llm"))]
        {
            let _ = (prompt, gen_config);
            Err(TrtError::FeatureDisabled)
        }

        #[cfg(feature = "trt-llm")]
        {
            if !self.loaded {
                return Err(TrtError::Init("Engine not loaded".into()));
            }
            // NOTE: tokio::sync::mpsc channel, C++ callbacks push chunks
            let _ = (prompt, gen_config);
            Err(TrtError::Init("TRT streaming FFI not implemented".into()))
        }
    }

    /// Suggest routing priority based on model config.
    /// Think → GPU, Dream → CPU NUMA, Embed → GPU (small model).
    pub fn suggest_priority(&self, priority: BatchPriority) -> BatchPriority {
        priority
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_mode_engine_creates() {
        let engine = TrtEngine::from_onnx_or_engine("/models/test", TrtConfig::default())
            .await
            .unwrap();
        assert!(
            !engine.is_available(),
            "Stub mode should report unavailable"
        );
        assert_eq!(engine.model_path(), "/models/test");
    }

    #[tokio::test]
    async fn stub_mode_generate_returns_feature_disabled() {
        let engine = TrtEngine::from_onnx_or_engine("/models/test", TrtConfig::default())
            .await
            .unwrap();
        let result = engine.generate("test", GenerationConfig::default()).await;
        assert!(matches!(result, Err(TrtError::FeatureDisabled)));
    }

    #[tokio::test]
    async fn stub_mode_stream_returns_feature_disabled() {
        let engine = TrtEngine::from_onnx_or_engine("/models/test", TrtConfig::default())
            .await
            .unwrap();
        let result = engine
            .generate_stream("test", GenerationConfig::default())
            .await;
        assert!(matches!(result, Err(TrtError::FeatureDisabled)));
    }

    #[test]
    fn suggest_priority_passthrough() {
        let config = TrtConfig::default();
        let engine = TrtEngine {
            config,
            model_path: "/models/test".into(),
            loaded: false,
        };
        assert_eq!(
            engine.suggest_priority(BatchPriority::Think),
            BatchPriority::Think
        );
        assert_eq!(
            engine.suggest_priority(BatchPriority::Dream),
            BatchPriority::Dream
        );
    }
}
