//! Concept — the atomic unit of memory.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A concept stored in the memory graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Concept {
    pub label: String,
    pub kind: ConceptKind,
    pub created: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub access_count: u32,
    pub metadata: HashMap<String, String>,
}

impl Concept {
    /// Create a new concept with timestamps initialised to now.
    pub fn new(label: impl Into<String>, kind: ConceptKind) -> Self {
        let now = Utc::now();
        Self {
            label: label.into(),
            kind,
            created: now,
            last_accessed: now,
            access_count: 0,
            metadata: HashMap::new(),
        }
    }

    /// Record an access — bump count and timestamp.
    pub fn touch(&mut self) {
        self.access_count += 1;
        self.last_accessed = Utc::now();
    }
}

/// The kind of concept — drives downstream behaviour (decay, visualisation, etc.)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConceptKind {
    Fact,
    Event,
    Skill,
    Person,
    Project,
    Neural, // HNN-derived concept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concept_creation() {
        let c = Concept::new("rust", ConceptKind::Skill);
        assert_eq!(c.label, "rust");
        assert_eq!(c.kind, ConceptKind::Skill);
        assert_eq!(c.access_count, 0);
    }

    #[test]
    fn concept_touch() {
        let mut c = Concept::new("test", ConceptKind::Fact);
        c.touch();
        assert_eq!(c.access_count, 1);
        c.touch();
        assert_eq!(c.access_count, 2);
    }

    #[test]
    fn concept_serialization_roundtrip() {
        let c = Concept::new("serde-test", ConceptKind::Person);
        let json = serde_json::to_string(&c).unwrap();
        let c2: Concept = serde_json::from_str(&json).unwrap();
        assert_eq!(c.label, c2.label);
        assert_eq!(c.kind, c2.kind);
    }
}