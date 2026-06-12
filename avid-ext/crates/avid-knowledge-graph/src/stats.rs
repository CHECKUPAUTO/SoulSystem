//! Graph statistics for the AVID knowledge graph.
//!
//! Provides summary metrics about entities, relationships, and graph structure.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::entity::EntityType;

/// Summary statistics for a knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    /// Total number of entities in the graph.
    pub total_entities: u64,
    /// Total number of relationships in the graph.
    pub total_relationships: u64,
    /// Breakdown of entity counts by type.
    pub entity_types: HashMap<String, u64>,
    /// Average number of relationships per entity.
    pub avg_degree: f32,
    /// Graph density: (2*E) / (N*(N-1)) for N>1, else 0.
    pub density: f32,
}

impl GraphStats {
    /// Compute stats from raw counts.
    ///
    /// `total_entities` and `total_relationships` are direct counts.
    /// `entity_type_counts` maps entity type names to their counts.
    #[must_use]
    pub fn compute(
        total_entities: u64,
        total_relationships: u64,
        entity_type_counts: HashMap<String, u64>,
    ) -> Self {
        let avg_degree = if total_entities > 0 {
            (total_relationships as f32) / (total_entities as f32)
        } else {
            0.0
        };

        let density = if total_entities > 1 {
            let n = total_entities as f32;
            (2.0 * total_relationships as f32) / (n * (n - 1.0))
        } else {
            0.0
        };

        Self {
            total_entities,
            total_relationships,
            entity_types: entity_type_counts,
            avg_degree,
            density,
        }
    }
}

impl From<EntityType> for String {
    fn from(et: EntityType) -> Self {
        et.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_graph_stats() {
        let s = GraphStats::compute(0, 0, HashMap::new());
        assert_eq!(s.total_entities, 0);
        assert_eq!(s.total_relationships, 0);
        assert!((s.avg_degree - 0.0).abs() < f32::EPSILON);
        assert!((s.density - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn single_entity_stats() {
        let mut types = HashMap::new();
        types.insert("concept".to_string(), 1);
        let s = GraphStats::compute(1, 0, types);
        assert_eq!(s.total_entities, 1);
        assert!((s.avg_degree - 0.0).abs() < f32::EPSILON);
        assert!((s.density - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn simple_graph_stats() {
        let mut types = HashMap::new();
        types.insert("concept".to_string(), 2);
        types.insert("tool".to_string(), 1);
        let s = GraphStats::compute(3, 2, types);
        assert_eq!(s.total_entities, 3);
        assert_eq!(s.total_relationships, 2);
        // avg_degree = 2/3 ≈ 0.666
        assert!(s.avg_degree > 0.6);
        // density = (2*2) / (3*2) = 4/6 ≈ 0.666
        assert!(s.density > 0.6);
    }
}
