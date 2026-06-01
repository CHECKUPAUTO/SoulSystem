//! Migration — schema versioning for dynamic field mutations.
//!
//! Tracks version changes to Neuron, Synapse, and BrainModule structures
//! and provides state migration when fields are added/removed at runtime
//! via the genome's self-modification system.

#![allow(dead_code)]

use std::collections::HashMap;

/// Current schema version of the codebase.
/// Incremented when the base struct definitions change.
pub const CURRENT_SCHEMA_VERSION: u64 = 1;

/// A field definition within a schema version.
#[derive(Clone, Debug)]
pub struct FieldDef {
    pub name: String,
    pub field_type: FieldType,
    pub default: f64,
    pub description: String,
}

/// The type of a dynamic field.
#[derive(Clone, Debug)]
pub enum FieldType {
    Float,
    Int,
    Bool,
}

/// Schema version info for a target type (Neuron, Synapse, BrainModule).
#[derive(Clone, Debug)]
pub struct SchemaVersion {
    /// Version number
    pub version: u64,
    /// Field definitions for this version
    pub fields: HashMap<String, FieldDef>,
}

impl SchemaVersion {
    /// Detect the current schema version from a set of extensions maps.
    /// Checks if any extensions exist — if not, returns v0 (pre-migration).
    pub fn detect(neurons: &[impl HasExtensions], synapses: &[impl HasExtensions]) -> u64 {
        let has_neuron_ext = neurons.iter().any(|n| !n.get_extensions().is_empty());
        let has_syn_ext = synapses.iter().any(|s| !s.get_extensions().is_empty());
        if has_neuron_ext || has_syn_ext {
            CURRENT_SCHEMA_VERSION
        } else {
            0
        }
    }
}

/// Trait for types that have extension maps.
pub trait HasExtensions {
    fn get_extensions(&self) -> &HashMap<String, f64>;
    fn get_extensions_mut(&mut self) -> &mut HashMap<String, f64>;
}

/// Apply migrations to bring the brain state up to the current version.
/// Returns a list of changes made.
pub fn apply_migration(
    _neurons: &mut [impl HasExtensions],
    _synapses: &mut [impl HasExtensions],
    current_version: u64,
) -> Vec<String> {
    let mut changes = Vec::new();

    if current_version < 1 {
        // Version 0 -> 1: initialize empty extension maps (no-op since
        // we use #[serde(default)], but we bump the conceptual version).
        changes.push("Schema v0 -> v1: initialized extension maps (no data changes needed)".into());
    }

    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestNeuron {
        extensions: HashMap<String, f64>,
    }

    impl HasExtensions for TestNeuron {
        fn get_extensions(&self) -> &HashMap<String, f64> {
            &self.extensions
        }
        fn get_extensions_mut(&mut self) -> &mut HashMap<String, f64> {
            &mut self.extensions
        }
    }

    struct TestSynapse {
        extensions: HashMap<String, f64>,
    }

    impl HasExtensions for TestSynapse {
        fn get_extensions(&self) -> &HashMap<String, f64> {
            &self.extensions
        }
        fn get_extensions_mut(&mut self) -> &mut HashMap<String, f64> {
            &mut self.extensions
        }
    }

    #[test]
    fn test_detect_v0() {
        #[derive(Default)]
        struct T(HashMap<String, f64>);
        impl HasExtensions for T {
            fn get_extensions(&self) -> &HashMap<String, f64> { &self.0 }
            fn get_extensions_mut(&mut self) -> &mut HashMap<String, f64> { &mut self.0 }
        }
        let neurons: Vec<T> = vec![T::default()];
        let synapses: Vec<T> = vec![];
        assert_eq!(SchemaVersion::detect(&neurons, &synapses), 0);
    }

    #[test]
    fn test_detect_v1() {
        let mut n = TestNeuron { extensions: HashMap::new() };
        n.extensions.insert("test".into(), 1.0);
        let neurons = vec![n];
        let synapses: Vec<TestSynapse> = vec![];
        assert_eq!(SchemaVersion::detect(&neurons, &synapses), CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn test_migration_v0_to_v1() {
        let mut neurons = vec![TestNeuron { extensions: HashMap::new() }];
        let mut synapses = vec![TestSynapse { extensions: HashMap::new() }];
        let changes = apply_migration(&mut neurons, &mut synapses, 0);
        assert!(!changes.is_empty(), "Migration should report changes");
    }

    #[test]
    fn test_migration_idempotent() {
        let mut neurons = vec![TestNeuron { extensions: HashMap::new() }];
        let mut synapses = vec![TestSynapse { extensions: HashMap::new() }];
        let _c1 = apply_migration(&mut neurons, &mut synapses, 0);
        let c2 = apply_migration(&mut neurons, &mut synapses, 1);
        assert!(c2.iter().all(|c| c.contains("no data")), "Second migration should be no-op");
    }
}
