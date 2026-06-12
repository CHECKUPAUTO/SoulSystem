//! Ollama embedding client — wrapper around `soulsystem-common::OllamaClient`.

pub use soulsystem_common::llm_client::OllamaClient;

/// Type alias pour compatibilité avec l'ancien API.
pub type OllamaEmbeddingClient = OllamaClient;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_construction() {
        let c = OllamaEmbeddingClient::new("http://localhost:11434", "nomic-embed-text");
        assert_eq!(c.base_url(), "http://localhost:11434");
        assert_eq!(c.model_name(), "nomic-embed-text");
    }

    #[test]
    fn client_trims_trailing_slash() {
        let c = OllamaEmbeddingClient::new("http://localhost:11434/", "test");
        assert_eq!(c.base_url(), "http://localhost:11434");
    }
}
