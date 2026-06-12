//! Image embedding types and Ollama-backed embedding generation.

use serde::{Deserialize, Serialize};

/// Available embedding backends.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EmbeddingBackend {
    /// CLIP ViT-B/32 — general purpose image embeddings (768-dim).
    ClipVitB32,
    /// Moondream2 — captioning + embedding model.
    Moondream2,
    /// Llava — multimodal LLM with embedding extraction.
    Llava,
    /// Custom model specified by name.
    Custom(String),
}

impl EmbeddingBackend {
    /// Ollama model tag for this backend.
    pub fn model_tag(&self) -> &str {
        match self {
            EmbeddingBackend::ClipVitB32 => "clip-vit-b-32",
            EmbeddingBackend::Moondream2 => "moondream2",
            EmbeddingBackend::Llava => "llava",
            EmbeddingBackend::Custom(name) => name.as_str(),
        }
    }
}

/// An image embedding result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageEmbedding {
    /// The embedding vector.
    pub vector: Vec<f32>,
    /// Dimension of the embedding.
    pub dimension: usize,
    /// Backend that produced this embedding.
    pub backend: EmbeddingBackend,
    /// Base64-encoded source image (for caching).
    pub image_hash: String,
    /// Processing time in milliseconds.
    pub duration_ms: u64,
}

impl ImageEmbedding {
    /// Compute cosine similarity with another embedding.
    pub fn cosine_similarity(&self, other: &ImageEmbedding) -> f32 {
        if self.dimension != other.dimension {
            return 0.0;
        }
        let dot: f32 = self
            .vector
            .iter()
            .zip(other.vector.iter())
            .map(|(a, b)| a * b)
            .sum();
        let na: f32 = self.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        let nb: f32 = other.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if na * nb == 0.0 {
            0.0
        } else {
            dot / (na * nb)
        }
    }
}
