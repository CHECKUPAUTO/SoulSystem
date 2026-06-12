//! Relationship — an edge connecting two entities in the AVID knowledge graph.
//!
//! Relationships carry a semantic type (e.g. DependsOn, Implements),
//! a weight, and optional metadata.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// The semantic type of a relationship between two entities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelType {
    /// Source depends on target (e.g. a task needs a tool).
    DependsOn,
    /// Source implements target (e.g. code implements a spec).
    Implements,
    /// Source uses target (e.g. an agent uses a tool).
    Uses,
    /// Source is similar to target.
    SimilarTo,
    /// Source contradicts target (logical conflict).
    Contradicts,
    /// Source precedes target (temporal ordering).
    Precedes,
    /// Source contains target (composition).
    Contains,
}

impl RelType {
    /// Parse from a string (case-insensitive).
    ///
    /// Returns `None` if the string doesn't match any variant.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "depends_on" | "dependson" => Some(Self::DependsOn),
            "implements" => Some(Self::Implements),
            "uses" => Some(Self::Uses),
            "similar_to" | "similarto" => Some(Self::SimilarTo),
            "contradicts" => Some(Self::Contradicts),
            "precedes" => Some(Self::Precedes),
            "contains" => Some(Self::Contains),
            _ => None,
        }
    }

    /// All relationship type variants as a slice.
    #[must_use]
    pub fn all() -> &'static [Self] {
        &[
            Self::DependsOn,
            Self::Implements,
            Self::Uses,
            Self::SimilarTo,
            Self::Contradicts,
            Self::Precedes,
            Self::Contains,
        ]
    }
}

impl std::fmt::Display for RelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DependsOn => write!(f, "depends_on"),
            Self::Implements => write!(f, "implements"),
            Self::Uses => write!(f, "uses"),
            Self::SimilarTo => write!(f, "similar_to"),
            Self::Contradicts => write!(f, "contradicts"),
            Self::Precedes => write!(f, "precedes"),
            Self::Contains => write!(f, "contains"),
        }
    }
}

/// A directed edge between two entities in the knowledge graph.
///
/// The relationship goes from `source_id` to `target_id` and carries
/// a semantic type, weight, and optional metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    /// Entity id where the relationship originates.
    pub source_id: String,
    /// Entity id where the relationship points.
    pub target_id: String,
    /// The kind of relationship.
    pub rel_type: RelType,
    /// Importance/strength from 0.0 to 1.0.
    pub weight: f32,
    /// Arbitrary key-value metadata.
    pub metadata: HashMap<String, String>,
}

impl Relationship {
    /// Create a new relationship with default weight 0.5.
    #[must_use]
    pub fn new(source_id: String, target_id: String, rel_type: RelType) -> Self {
        Self {
            source_id,
            target_id,
            rel_type,
            weight: 0.5,
            metadata: HashMap::new(),
        }
    }

    /// Builder-style: set the weight, clamped to [0.0, 1.0].
    #[must_use]
    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = weight.clamp(0.0, 1.0);
        self
    }

    /// Builder-style: set a metadata entry.
    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rel_type_from_str() {
        assert_eq!(RelType::parse("depends_on"), Some(RelType::DependsOn));
        assert_eq!(RelType::parse("dependson"), Some(RelType::DependsOn));
        assert_eq!(RelType::parse("USES"), Some(RelType::Uses));
        assert_eq!(RelType::parse("similar_to"), Some(RelType::SimilarTo));
        assert_eq!(RelType::parse("unknown"), None);
    }

    #[test]
    fn rel_type_display() {
        assert_eq!(RelType::DependsOn.to_string(), "depends_on");
        assert_eq!(RelType::Contains.to_string(), "contains");
    }

    #[test]
    fn relationship_builders() {
        let r = Relationship::new("a".into(), "b".into(), RelType::DependsOn)
            .with_weight(0.8)
            .with_metadata("source", "manual");
        assert_eq!(r.source_id, "a");
        assert_eq!(r.target_id, "b");
        assert!((r.weight - 0.8).abs() < f32::EPSILON);
        assert_eq!(r.metadata.get("source").unwrap(), "manual");
    }

    #[test]
    fn weight_clamped() {
        let r = Relationship::new("a".into(), "b".into(), RelType::Uses).with_weight(1.5);
        assert!((r.weight - 1.0).abs() < f32::EPSILON);
    }
}
