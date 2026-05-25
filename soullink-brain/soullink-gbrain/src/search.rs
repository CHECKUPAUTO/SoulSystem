/*
 * AVID Standard Header
 * Project: SoulLink
 * Module: soullink-gbrain
 * Author: SoulLink Team
 */

//! Hybrid Search implementation (BM25 + Vector + Graph Boost).

use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};
use anyhow::Result;
use crate::storage::Database;
use reqwest::Client;

/// BM25 index for entity context search.
#[derive(Debug, Serialize, Deserialize)]
pub struct Bm25Index {
    docs: HashMap<String, Bm25Doc>,
    df: HashMap<String, usize>,
    inverted_index: HashMap<String, Vec<String>>,
    total_dl: usize,
    avgdl: f64,
    k1: f64,
    b: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Bm25Doc {
    id: String,
    tf: HashMap<String, f64>,
    dl: usize,
}

impl Bm25Index {
    pub fn new() -> Self {
        Self {
            docs: HashMap::new(),
            df: HashMap::new(),
            inverted_index: HashMap::new(),
            total_dl: 0,
            avgdl: 0.0,
            k1: 1.2,
            b: 0.75,
        }
    }

    pub fn add(&mut self, id: &str, text: &str) {
        let tokens: Vec<String> = text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        let dl = tokens.len();
        let mut tf = HashMap::new();
        for t in &tokens {
            *tf.entry(t.clone()).or_insert(0.0) += 1.0;
        }

        // Bolt ⚡: Handle updates by removing old document stats if ID already exists.
        // This makes rebuilding the index O(N) instead of O(N^2).
        if let Some(old_doc) = self.docs.remove(id) {
            self.total_dl -= old_doc.dl;
            for t in old_doc.tf.keys() {
                if let Some(count) = self.df.get_mut(t) {
                    if *count > 0 { *count -= 1; }
                }
                // Bolt ⚡: Prune inverted index to prevent memory leak/bloat.
                if let Some(entries) = self.inverted_index.get_mut(t) {
                    entries.retain(|doc_id| doc_id != id);
                }
            }
        }

        for t in tf.keys() {
            *self.df.entry(t.clone()).or_insert(0) += 1;
            // Bolt ⚡: Update inverted index for O(1) per token document lookup.
            let entries = self.inverted_index.entry(t.clone()).or_default();
            entries.push(id.to_string());
        }

        self.total_dl += dl;
        self.docs.insert(id.to_string(), Bm25Doc { id: id.to_string(), tf, dl });

        self.avgdl = if !self.docs.is_empty() {
            self.total_dl as f64 / self.docs.len() as f64
        } else {
            0.0
        };
    }

    pub fn search(&self, query: &str, top_k: usize) -> Vec<(String, f64)> {
        if self.docs.is_empty() { return Vec::new(); }
        let query_tokens: Vec<String> = query.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        if query_tokens.is_empty() { return Vec::new(); }

        // Bolt ⚡: Pre-calculate IDF for each unique query token to avoid redundant log calls.
        let n = self.docs.len() as f64;
        let mut idfs = HashMap::new();
        let mut target_doc_ids = HashSet::new();

        for t in &query_tokens {
            if idfs.contains_key(t) { continue; }
            let df = *self.df.get(t).unwrap_or(&0) as f64;
            let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
            idfs.insert(t.clone(), idf);

            // Bolt ⚡: Only consider documents that contain at least one query token.
            if let Some(doc_ids) = self.inverted_index.get(t) {
                for doc_id in doc_ids {
                    target_doc_ids.insert(doc_id);
                }
            }
        }

        let mut scores = Vec::new();
        let avgdl = self.avgdl.max(1.0);
        let k1_plus_1 = self.k1 + 1.0;
        let one_minus_b = 1.0 - self.b;

        for doc_id in target_doc_ids {
            if let Some(doc) = self.docs.get(doc_id) {
                let mut score = 0.0;
                let doc_dl_factor = self.b * (doc.dl as f64 / avgdl);
                for t in &query_tokens {
                    if let Some(&tf) = doc.tf.get(t) {
                        if let Some(&idf) = idfs.get(t) {
                            let num = tf * k1_plus_1;
                            let den = tf + self.k1 * (one_minus_b + doc_dl_factor);
                            score += idf * num / den;
                        }
                    }
                }
                if score > 0.0 {
                    scores.push((doc.id.clone(), score));
                }
            }
        }

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(top_k);
        scores
    }
}

pub struct HybridSearcher {
    db: Database,
    bm25: Bm25Index,
    client: Client,
    memory_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct MemorySearchResponse {
    label: String,
    score: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchHit {
    pub entity_id: String,
    pub score: f64,
    pub name: String,
    pub entity_type: String,
}

impl HybridSearcher {
    pub fn new(db: Database, memory_url: String) -> Self {
        Self {
            db,
            bm25: Bm25Index::new(),
            client: Client::new(),
            memory_url,
        }
    }

    pub fn rebuild_bm25(&mut self) -> Result<()> {
        let entities = self.db.get_entities()?;
        let mut new_bm25 = Bm25Index::new();
        for e in entities {
            let content = format!("{} {} {}", e.name, e.entity_type, e.source_page);
            new_bm25.add(&e.id, &content);
        }
        self.bm25 = new_bm25;
        Ok(())
    }

    pub async fn search(&self, query: &str, top_k: usize) -> Result<Vec<SearchHit>> {
        // 1. Vector Search (from port 9030)
        let vector_results = self.fetch_vector_results(query, top_k).await.unwrap_or_default();

        // 2. BM25 Search
        let bm25_results = self.bm25.search(query, top_k);

        // 3. Combined Ranking
        let mut combined: HashMap<String, f64> = HashMap::new();

        // Vector: 0.4
        for res in vector_results {
            // Memory store results might be labels. We try to map them to entities.
            // If label is "Person:Garry Tan", it matches our entity ID.
            *combined.entry(res.label).or_insert(0.0) += 0.4 * res.score as f64;
        }

        // BM25: 0.3
        let max_bm25 = bm25_results.get(0).map(|r| r.1).unwrap_or(1.0).max(1.0);
        for (id, score) in bm25_results {
            *combined.entry(id).or_insert(0.0) += 0.3 * (score / max_bm25);
        }

        // 4. Graph Boost: 0.3
        // Bolt ⚡: Use optimized counting and direct lookups to avoid O(N*M) bottlenecks.
        // Also use batched lookups to avoid N+1 queries.
        let ids: Vec<String> = combined.keys().cloned().collect();
        let entities_with_counts = self.db.get_entities_with_edge_counts(&ids)?;

        let mut final_hits = Vec::new();
        for (entity, edge_count) in entities_with_counts {
            let score = combined.get(&entity.id).unwrap_or(&0.0);
            let boost = (edge_count as f64 * 0.1).min(1.0);
            let final_score = score + 0.3 * boost;

            final_hits.push(SearchHit {
                entity_id: entity.id,
                score: final_score,
                name: entity.name,
                entity_type: entity.entity_type,
            });
        }

        final_hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        final_hits.truncate(top_k);

        Ok(final_hits)
    }

    async fn fetch_vector_results(&self, query: &str, top_k: usize) -> Result<Vec<MemorySearchResponse>> {
        let url = format!("{}/api/search?q={}&top_k={}", self.memory_url, query, top_k);
        let resp = self.client.get(url).send().await?;
        let results: Vec<MemorySearchResponse> = resp.json().await?;
        Ok(results)
    }
}

#[cfg(test)]
mod bm25_tests {
    use super::*;

    #[test]
    fn test_bm25_index_and_search() {
        let mut index = Bm25Index::new();
        index.add("doc1", "Garry Tan is the CEO of Y Combinator");
        index.add("doc2", "The CEO of OpenAI is Sam Altman");
        index.add("doc3", "San Francisco is a city in California");

        // Test basic search
        let results = index.search("CEO Garry", 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].0, "doc1");

        let results = index.search("Sam", 10);
        assert_eq!(results[0].0, "doc2");

        let results = index.search("San Francisco", 10);
        assert_eq!(results[0].0, "doc3");

        // Test incremental maintenance
        assert!(index.total_dl > 0);
        assert!(index.avgdl > 0.0);
        assert!(!index.inverted_index.is_empty());

        // Test no results
        let results = index.search("unknown", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_bm25_update_document() {
        let mut index = Bm25Index::new();
        index.add("doc1", "initial content");

        // Update document
        index.add("doc1", "updated version");

        assert_eq!(index.docs.len(), 1);
        assert_eq!(index.total_dl, 2);
        assert_eq!(index.avgdl, 2.0);
        assert_eq!(*index.df.get("updated").unwrap(), 1);
        assert_eq!(*index.df.get("initial").unwrap_or(&0), 0);
    }
}
