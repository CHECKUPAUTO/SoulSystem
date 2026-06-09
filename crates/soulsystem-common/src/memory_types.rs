//! Memory Types — Types partagés pour les résultats de recherche mémoire.

use serde::{Deserialize, Serialize};

/// Résultat de recherche mémoire (format unifié).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryHit {
    pub text: String,
    pub score: f32,
    pub source: String,
    #[serde(default)]
    pub timestamp: Option<String>,
}

/// Résultat de recherche vectorielle (format alternatif).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub content: String,
    pub score: f64,
    pub source: String,
}

impl From<MemoryHit> for SearchResult {
    fn from(hit: MemoryHit) -> Self {
        Self {
            content: hit.text,
            score: hit.score as f64,
            source: hit.source,
        }
    }
}

impl From<SearchResult> for MemoryHit {
    fn from(result: SearchResult) -> Self {
        Self {
            text: result.content,
            score: result.score as f32,
            source: result.source,
            timestamp: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hit_to_search_result() {
        let hit = MemoryHit {
            text: "hello".into(),
            score: 0.9,
            source: "test".into(),
            timestamp: None,
        };
        let sr: SearchResult = hit.into();
        assert_eq!(sr.content, "hello");
        assert!((sr.score - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_search_result_to_hit() {
        let sr = SearchResult {
            content: "world".into(),
            score: 0.75,
            source: "test".into(),
        };
        let hit: MemoryHit = sr.into();
        assert_eq!(hit.text, "world");
        assert!((hit.score - 0.75).abs() < 0.01);
    }
}
