//! BM25 keyword search — hybrid search complement to vector similarity.
//!
//! Simple BM25 implementation for keyword matching on page chunks.
//! Combined with HNSW cosine similarity, this enables hybrid retrieval:
//! vector (semantic) + keyword (exact match) = better recall.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A document in the BM25 index.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Bm25Doc {
    id: String,
    tokens: Vec<String>,
    tf: HashMap<String, f64>,
    dl: usize,
}

/// BM25 index for keyword search.
///
/// Uses Okapi BM25 scoring: score(q, d) = Σ IDF(qi) · (f(qi,d) · (k1+1)) / (f(qi,d) + k1·(1-b+b·|d|/avgdl))
#[derive(Debug, Serialize, Deserialize)]
pub struct Bm25Index {
    docs: HashMap<String, Bm25Doc>,
    df: HashMap<String, usize>,
    avgdl: f64,
    k1: f64,
    b: f64,
    total_docs: usize,
}

impl Default for Bm25Index {
    fn default() -> Self {
        Self::new()
    }
}

impl Bm25Index {
    pub fn new() -> Self {
        Self {
            docs: HashMap::new(),
            df: HashMap::new(),
            avgdl: 0.0,
            k1: 1.2,
            b: 0.75,
            total_docs: 0,
        }
    }

    pub fn with_params(k1: f64, b: f64) -> Self {
        Self {
            docs: HashMap::new(),
            df: HashMap::new(),
            avgdl: 0.0,
            k1,
            b,
            total_docs: 0,
        }
    }

    /// Tokenize text into lowercase words.
    fn tokenize(text: &str) -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty() && s.len() > 1)
            .map(|s| s.to_string())
            .collect()
    }

    /// Add a document to the index.
    pub fn add(&mut self, id: impl Into<String>, text: &str) {
        let id = id.into();
        let tokens = Self::tokenize(text);
        let dl = tokens.len();
        let mut tf: HashMap<String, f64> = HashMap::new();

        // Count term frequencies
        for token in &tokens {
            *tf.entry(token.clone()).or_insert(0.0) += 1.0;
        }

        // Update document frequency
        for term in tf.keys() {
            *self.df.entry(term.clone()).or_insert(0) += 1;
        }

        self.docs.insert(id.clone(), Bm25Doc { id, tokens, tf, dl });
        self.total_docs = self.docs.len();

        // Recompute avgdl
        let total_len: usize = self.docs.values().map(|d| d.dl).sum();
        self.avgdl = if self.total_docs > 0 {
            total_len as f64 / self.total_docs as f64
        } else {
            0.0
        };
    }

    /// Compute IDF for a term.
    fn idf(&self, term: &str) -> f64 {
        let df = *self.df.get(term).unwrap_or(&0) as f64;
        let n = self.total_docs as f64;
        ((n - df + 0.5) / (df + 0.5) + 1.0).max(0.0)
    }

    /// Search for documents matching the query, returning (doc_id, score) pairs.
    pub fn search(&self, query: &str, top_k: usize) -> Vec<(String, f64)> {
        let query_tokens = Self::tokenize(query);
        let mut scores: HashMap<String, f64> = HashMap::new();

        for doc in self.docs.values() {
            let mut score = 0.0;
            for term in &query_tokens {
                let tf = doc.tf.get(term).copied().unwrap_or(0.0);
                if tf == 0.0 {
                    continue;
                }

                let idf = self.idf(term);
                let numerator = tf * (self.k1 + 1.0);
                let denominator =
                    tf + self.k1 * (1.0 - self.b + self.b * doc.dl as f64 / self.avgdl.max(1.0));
                score += idf * numerator / denominator;
            }
            if score > 0.0 {
                scores.insert(doc.id.clone(), score);
            }
        }

        let mut results: Vec<(String, f64)> = scores.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        results
    }

    /// Number of documents in the index.
    pub fn len(&self) -> usize {
        self.docs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bm25_basic_search() {
        let mut index = Bm25Index::new();
        index.add("doc1", "The quick brown fox jumps over the lazy dog");
        index.add("doc2", "A fast cat runs through the garden");
        index.add("doc3", "The fox is quick and clever");

        let results = index.search("quick fox", 2);
        assert!(!results.is_empty());
        // doc1 and doc3 should score highest for "quick fox"
        assert!(results[0].0 == "doc1" || results[0].0 == "doc3");
    }

    #[test]
    fn bm25_empty_query() {
        let mut index = Bm25Index::new();
        index.add("doc1", "Hello world");
        let results = index.search("", 5);
        assert!(results.is_empty());
    }

    #[test]
    fn bm25_no_match() {
        let mut index = Bm25Index::new();
        index.add("doc1", "Hello world");
        let results = index.search("quantum physics", 5);
        assert!(results.is_empty());
    }

    #[test]
    fn bm25_idf_rare_terms_score_higher() {
        let mut index = Bm25Index::new();
        // Add 10 docs about cats
        for i in 0..10 {
            index.add(format!("cat-{}", i), "cats are fluffy animals that purr");
        }
        // Add 1 doc about quantum
        index.add(
            "quantum-1",
            "quantum mechanics describes subatomic particles",
        );

        let results = index.search("quantum", 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].0, "quantum-1");
    }

    #[test]
    fn bm25_len_and_empty() {
        let mut index = Bm25Index::new();
        assert!(index.is_empty());
        index.add("doc1", "content");
        assert_eq!(index.len(), 1);
        assert!(!index.is_empty());
    }
}
