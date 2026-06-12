//! Entity — a node in the AVID knowledge graph.
//!
//! Entities represent concepts, tools, tasks, patterns, agents, or documents
//! tracked by the knowledge graph. Each entity carries typed properties,
//! timestamps, and an importance weight.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// The type of entity in the knowledge graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    /// Abstract ideas or knowledge domains.
    Concept,
    /// Executable tools, binaries, or scripts.
    Tool,
    /// Tasks or work items.
    Task,
    /// Recurring patterns or templates.
    Pattern,
    /// AI or human agents.
    Agent,
    /// Documents, specs, or textual artifacts.
    Document,
}

impl EntityType {
    /// Parse from a string (case-insensitive).
    ///
    /// Returns `None` if the string doesn't match any variant.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "concept" => Some(Self::Concept),
            "tool" => Some(Self::Tool),
            "task" => Some(Self::Task),
            "pattern" => Some(Self::Pattern),
            "agent" => Some(Self::Agent),
            "document" => Some(Self::Document),
            _ => None,
        }
    }

    /// All entity type variants as a slice.
    #[must_use]
    pub fn all() -> &'static [Self] {
        &[
            Self::Concept,
            Self::Tool,
            Self::Task,
            Self::Pattern,
            Self::Agent,
            Self::Document,
        ]
    }
}

impl std::fmt::Display for EntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Concept => write!(f, "concept"),
            Self::Tool => write!(f, "tool"),
            Self::Task => write!(f, "task"),
            Self::Pattern => write!(f, "pattern"),
            Self::Agent => write!(f, "agent"),
            Self::Document => write!(f, "document"),
        }
    }
}

/// A node in the AVID knowledge graph.
///
/// Each entity has a unique UUID, a name, a type, arbitrary string properties,
/// creation and update timestamps, and an importance weight between 0.0 and 1.0.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// Unique identifier (UUID v4).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// The kind of entity.
    pub entity_type: EntityType,
    /// Arbitrary key-value metadata.
    pub properties: HashMap<String, String>,
    /// Unix timestamp of creation (seconds).
    pub created_at: i64,
    /// Unix timestamp of last update (seconds).
    pub updated_at: i64,
    /// Importance weight from 0.0 (least) to 1.0 (most).
    pub weight: f32,
}

impl Entity {
    /// Create a new entity with a random UUID, current timestamp, and default weight.
    #[must_use]
    pub fn new(name: String, entity_type: EntityType) -> Self {
        let now = now_ts();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            entity_type,
            properties: HashMap::new(),
            created_at: now,
            updated_at: now,
            weight: 0.5,
        }
    }

    /// Builder-style: set a property.
    #[must_use]
    pub fn with_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }

    /// Builder-style: set multiple properties.
    #[must_use]
    pub fn with_properties(
        mut self,
        props: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        for (k, v) in props {
            self.properties.insert(k.into(), v.into());
        }
        self
    }

    /// Builder-style: set the importance weight, clamped to [0.0, 1.0].
    #[must_use]
    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = weight.clamp(0.0, 1.0);
        self
    }
}

/// Current unix timestamp in seconds.
fn now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_new_has_uuid_and_timestamps() {
        let e = Entity::new("test".into(), EntityType::Concept);
        assert!(!e.id.is_empty());
        assert_eq!(e.name, "test");
        assert_eq!(e.entity_type, EntityType::Concept);
        assert!(e.created_at > 0);
        assert_eq!(e.created_at, e.updated_at);
        assert!((e.weight - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn entity_builders() {
        let e = Entity::new("tool".into(), EntityType::Tool)
            .with_property("lang", "rust")
            .with_properties([("version", "1.0"), ("author", "clawd")])
            .with_weight(0.9);
        assert_eq!(e.properties.get("lang").unwrap(), "rust");
        assert_eq!(e.properties.get("version").unwrap(), "1.0");
        assert_eq!(e.properties.get("author").unwrap(), "clawd");
        assert!((e.weight - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn weight_clamped() {
        let e = Entity::new("x".into(), EntityType::Concept).with_weight(2.0);
        assert!((e.weight - 1.0).abs() < f32::EPSILON);

        let e = Entity::new("x".into(), EntityType::Concept).with_weight(-0.5);
        assert!((e.weight - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn entity_type_from_str() {
        assert_eq!(EntityType::parse("concept"), Some(EntityType::Concept));
        assert_eq!(EntityType::parse("TOOL"), Some(EntityType::Tool));
        assert_eq!(EntityType::parse("unknown"), None);
    }

    #[test]
    fn entity_type_display() {
        assert_eq!(EntityType::Concept.to_string(), "concept");
        assert_eq!(EntityType::Agent.to_string(), "agent");
    }
}
