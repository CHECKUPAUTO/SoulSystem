/*
 * AVID Standard Header
 * Project: SoulLink
 * Module: soullink-gbrain
 * Author: SoulLink Team
 */

//! Hybrid Search implementation (BM25 + Vector + Graph Boost).

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use crate::storage::Database;
use reqwest::Client;

/// BM25 index for entity context search.
#[derive(Debug, Serialize, Deserialize)]
pub struct Bm25Index {
    docs: HashMap<String, Bm25Doc>,
    df: HashMap<String, usize>,
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

        for t in tf.keys() {
            *self.df.entry(t.clone()).or_insert(0) += 1;
        }

        self.docs.insert(id.to_string(), Bm25Doc { id: id.to_string(), tf, dl });

        let total_dl: usize = self.docs.values().map(|d| d.dl).sum();
        self.avgdl = if !self.docs.is_empty() { total_dl as f64 / self.docs.len() as f64 } else { 0.0 };
    }

    pub fn search(&self, query: &str, top_k: usize) -> Vec<(String, f64)> {
        if self.docs.is_empty() { return Vec::new(); }
        let query_tokens: Vec<String> = query.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        let mut scores = Vec::new();
        let n = self.docs.len() as f64;

        for doc in self.docs.values() {
            let mut score = 0.0;
            for t in &query_tokens {
                if let Some(&tf) = doc.tf.get(t) {
                    let df = *self.df.get(t).unwrap_or(&0) as f64;
                    let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
                    let num = tf * (self.k1 + 1.0);
                    let den = tf + self.k1 * (1.0 - self.b + self.b * doc.dl as f64 / self.avgdl.max(1.0));
                    score += idf * num / den;
                }
            }
            if score > 0.0 {
                scores.push((doc.id.clone(), score));
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
        let mut final_hits = Vec::new();
        for (id, score) in combined {
            let edge_count = self.db.get_edge_count_for_entity(&id).unwrap_or(0);
            let boost = (edge_count as f64 * 0.1).min(1.0);
            let final_score = score + 0.3 * boost;

            // Fetch entity info
            if let Some(entity) = self.db.get_entity_by_id(&id)? {
                final_hits.push(SearchHit {
                    entity_id: id,
                    score: final_score,
                    name: entity.name,
                    entity_type: entity.entity_type,
                });
            }
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
