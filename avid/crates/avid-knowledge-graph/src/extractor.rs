//! LLM-based entity and relationship extraction from natural language text.
//!
//! Uses the AVID LLM client to identify entities and their relationships
//! from arbitrary text, enabling automatic population of the knowledge graph.

use std::collections::HashMap;
use std::time::Duration;

use avid_core::llm::LlmClient;
use serde::Deserialize;
use tracing::warn;

use crate::entity::{Entity, EntityType};
use crate::relationship::{RelType, Relationship};

/// Errors that can occur during LLM extraction.
#[derive(Debug, thiserror::Error)]
pub enum ExtractionError {
    /// The LLM call failed.
    #[error("LLM error: {0}")]
    Llm(#[from] avid_core::llm::LlmError),

    /// The LLM response could not be parsed.
    #[error("JSON parse error: {0}")]
    Parse(String),
}

/// Expected structure of the LLM extraction response.
#[derive(Debug, Deserialize)]
struct ExtractionResponse {
    entities: Vec<ExtractedEntity>,
    relationships: Vec<ExtractedRelationship>,
}

#[derive(Debug, Deserialize)]
struct ExtractedEntity {
    name: String,
    #[serde(rename = "type")]
    entity_type: String,
    #[serde(default)]
    properties: HashMap<String, String>,
    #[serde(default = "default_weight")]
    weight: f32,
}

fn default_weight() -> f32 {
    0.5
}

#[derive(Debug, Deserialize)]
struct ExtractedRelationship {
    source: String,
    target: String,
    #[serde(rename = "type")]
    rel_type: String,
    #[serde(default = "default_weight")]
    weight: f32,
}

/// Extract entities and relationships from arbitrary text using an LLM.
///
/// The LLM is prompted to identify key entities (concepts, tools, tasks, patterns,
/// agents, documents) and their relationships (depends_on, implements, uses, etc.).
///
/// # Errors
///
/// Returns `ExtractionError` if the LLM call fails or the response cannot be parsed.
pub fn extract_from_text(
    llm: &LlmClient,
    text: &str,
) -> Result<Vec<(Entity, Vec<Relationship>)>, ExtractionError> {
    let system = r#"You are a knowledge graph extraction engine. 
Analyze the given text and extract entities and relationships.

Entities have: name, type (concept/tool/task/pattern/agent/document), optional properties (key-value), optional weight (0-1).
Relationships have: source (entity name), target (entity name), type (depends_on/implements/uses/similar_to/contradicts/precedes/contains), optional weight (0-1).

Return ONLY valid JSON with no additional text:
{
  "entities": [
    {"name": "EntityName", "type": "concept", "properties": {"key": "value"}, "weight": 0.5}
  ],
  "relationships": [
    {"source": "SourceName", "target": "TargetName", "type": "depends_on", "weight": 0.8}
  ]
}

Rules:
- Entity names must be unique
- Relationship source/target must reference entity names present in the entities list
- Only extract relationships that are clearly stated or strongly implied
- Default weight is 0.5, use higher for more important/salient items"#;

    let user = format!("Extract entities and relationships from the following text:\n\n{text}");

    // Use tokio runtime if we're in async context, otherwise block_on
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    let response = rt.block_on(llm.chat(system, &user, true, Duration::from_secs(120)))?;

    let extraction: ExtractionResponse = serde_json::from_str(&response)
        .map_err(|e| ExtractionError::Parse(format!("failed to parse LLM response: {e}")))?;

    // Convert to domain types, building a name→id map
    let entities: Vec<Entity> = extraction
        .entities
        .into_iter()
        .map(|ex| {
            let entity_type = EntityType::parse(&ex.entity_type).unwrap_or(EntityType::Concept);
            Entity::new(ex.name.clone(), entity_type)
                .with_properties(ex.properties)
                .with_weight(ex.weight)
        })
        .collect();

    let name_to_id: HashMap<String, String> = entities
        .iter()
        .map(|e| (e.name.clone(), e.id.clone()))
        .collect();

    let mut entity_rels: HashMap<String, Vec<Relationship>> = entities
        .iter()
        .map(|e| (e.id.clone(), Vec::new()))
        .collect();

    for ex in extraction.relationships {
        let source_id = name_to_id.get(&ex.source);
        let target_id = name_to_id.get(&ex.target);

        match (source_id, target_id) {
            (Some(src), Some(tgt)) => {
                let rel_type = RelType::parse(&ex.rel_type).unwrap_or(RelType::Uses);
                let rel =
                    Relationship::new(src.clone(), tgt.clone(), rel_type).with_weight(ex.weight);
                entity_rels.entry(src.clone()).or_default().push(rel);
            }
            (None, _) => {
                warn!(
                    "relationship references unknown source entity '{}'",
                    ex.source
                );
            }
            (_, None) => {
                warn!(
                    "relationship references unknown target entity '{}'",
                    ex.target
                );
            }
        }
    }

    let result: Vec<(Entity, Vec<Relationship>)> = entities
        .into_iter()
        .map(|e| {
            let rels = entity_rels.remove(&e.id).unwrap_or_default();
            (e, rels)
        })
        .collect();

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that extraction response parsing works for a minimal valid payload.
    #[test]
    fn parse_extraction_response() {
        let json = r#"{
            "entities": [
                {"name": "Rust", "type": "tool", "weight": 0.9}
            ],
            "relationships": []
        }"#;

        let resp: ExtractionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.entities.len(), 1);
        assert_eq!(resp.entities[0].name, "Rust");
        assert_eq!(resp.entities[0].entity_type, "tool");
    }

    #[test]
    fn parse_with_relationships() {
        let json = r#"{
            "entities": [
                {"name": "TaskA", "type": "task"},
                {"name": "ToolB", "type": "tool"}
            ],
            "relationships": [
                {"source": "TaskA", "target": "ToolB", "type": "depends_on", "weight": 0.7}
            ]
        }"#;

        let resp: ExtractionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.entities.len(), 2);
        assert_eq!(resp.relationships.len(), 1);
        assert_eq!(resp.relationships[0].source, "TaskA");
        assert_eq!(resp.relationships[0].target, "ToolB");
    }
}
