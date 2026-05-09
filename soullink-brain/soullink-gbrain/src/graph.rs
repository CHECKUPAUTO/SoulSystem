/*
 * AVID Standard Header
 * Project: SoulLink
 * Module: soullink-gbrain
 * Author: SoulLink Team
 */

//! Auto-wiring Knowledge Graph logic.

use crate::extractor::{EntityType, ExtractedEntity};
use crate::storage::{Database, Entity, Edge};
use anyhow::Result;
use chrono::Utc;

pub struct KnowledgeGraph {
    db: Database,
}

impl KnowledgeGraph {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Auto-wire entities extracted from a page.
    pub fn auto_wire(&self, source_page: &str, extracted: &[ExtractedEntity], text: &str) -> Result<()> {
        let mut entities = Vec::new();

        // 1. Store entities first
        for ext in extracted {
            let entity = Entity {
                id: format!("{}:{}", ext.entity_type_str(), ext.name),
                entity_type: ext.entity_type_str().to_string(),
                name: ext.name.clone(),
                source_page: source_page.to_string(),
                confidence: ext.confidence,
                created_at: Utc::now(),
            };
            self.db.insert_entity(&entity)?;
            entities.push(entity);
        }

        // 2. Create links based on proximity and keywords
        for i in 0..entities.len() {
            for j in (i + 1)..entities.len() {
                let e1 = &entities[i];
                let e2 = &entities[j];
                let ext1 = &extracted[i];
                let ext2 = &extracted[j];

                // Check distance in text
                let dist = (ext1.offset as isize - ext2.offset as isize).abs();
                if dist < 100 {
                    // Extract context between them
                    let start = ext1.offset.min(ext2.offset);
                    let end = ext1.offset.max(ext2.offset) + 10;
                    let context = if end <= text.len() { &text[start..end] } else { &text[start..] };

                    let relation = self.detect_relation(e1, e2, context);

                    let edge = Edge {
                        source_id: e1.id.clone(),
                        target_id: e2.id.clone(),
                        relation_type: relation,
                        weight: 1.0 - (dist as f32 / 100.0).clamp(0.0, 0.9),
                        created_at: Utc::now(),
                    };
                    self.db.insert_edge(&edge)?;
                }
            }
        }

        Ok(())
    }

    fn detect_relation(&self, e1: &Entity, e2: &Entity, context: &str) -> String {
        let ctx = context.to_lowercase();

        if (e1.entity_type == "Person" && e2.entity_type == "Company") || (e1.entity_type == "Company" && e2.entity_type == "Person") {
            if ctx.contains("ceo") || ctx.contains("founder") || ctx.contains("works at") || ctx.contains("joined") {
                if ctx.contains("founder") { return "founded".to_string(); }
                return "works_at".to_string();
            }
            if ctx.contains("invested") || ctx.contains("investor") || ctx.contains("backed") {
                return "invested_in".to_string();
            }
            if ctx.contains("advisor") || ctx.contains("advises") {
                return "advises".to_string();
            }
        }

        if (e1.entity_type == "Person" && e2.entity_type == "Event") || (e1.entity_type == "Event" && e2.entity_type == "Person") {
            if ctx.contains("attended") || ctx.contains("spoke") || ctx.contains("at") {
                return "attended".to_string();
            }
        }

        "mentioned_in".to_string()
    }
}

impl ExtractedEntity {
    pub fn entity_type_str(&self) -> &str {
        match self.entity_type {
            EntityType::Person => "Person",
            EntityType::Company => "Company",
            EntityType::Role => "Role",
            EntityType::Event => "Event",
            EntityType::Location => "Location",
        }
    }
}
