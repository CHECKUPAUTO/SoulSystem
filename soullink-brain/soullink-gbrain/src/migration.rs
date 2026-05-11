/*
 * AVID Standard Header
 * Project: SoulLink
 * Module: soullink-gbrain
 * Author: SoulLink Team
 */

//! Migration logic from soullink-memory (9030) to gbrain.

use crate::extractor::EntityExtractor;
use crate::graph::KnowledgeGraph;
use crate::storage::Database;
use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize)]
struct MemoryConcept {
    label: String,
    metadata: std::collections::HashMap<String, String>,
}

pub struct Migrator {
    client: Client,
    memory_url: String,
    extractor: EntityExtractor,
    graph: KnowledgeGraph,
}

impl Migrator {
    pub fn new(memory_url: String, db: Database) -> Self {
        Self {
            client: Client::new(),
            memory_url,
            extractor: EntityExtractor::new(),
            graph: KnowledgeGraph::new(db),
        }
    }

    pub async fn migrate_all(&self) -> Result<usize> {
        // 1. Fetch all concepts from memory store
        let url = format!("{}/api/concepts", self.memory_url);
        let resp = self.client.get(url).send().await?;
        let concepts: Vec<MemoryConcept> = resp.json().await?;

        let mut count = 0;
        for concept in concepts {
            // 2. Extract entities from label and any metadata
            let text = format!("{} {}", concept.label, concept.metadata.values().cloned().collect::<Vec<_>>().join(" "));
            let extracted = self.extractor.extract(&text);

            if !extracted.is_empty() {
                // 3. Auto-wire and save to gbrain
                self.graph.auto_wire(&concept.label, &extracted, &text)?;
                count += extracted.len();
            }
        }

        Ok(count)
    }
}
