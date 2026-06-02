//! Sled persistence — serialize graph to key-value store.

use anyhow::{Context, Result};
use bincode;
use petgraph::stable_graph::NodeIndex;
use sled::Db;

use crate::concept::Concept;
use crate::graph::EdgeRecord;

// Key prefixes
const CONCEPT_PREFIX: &str = "concept:";
const EDGE_PREFIX: &str = "edge:";

/// Persist all nodes and edges to sled.
pub fn persist_graph(
    db: &Db,
    concepts: &[(NodeIndex, Concept)],
    edges: &[(NodeIndex, NodeIndex, f32)],
) -> Result<()> {
    // Clear old entries
    clear_prefix(db, CONCEPT_PREFIX)?;
    clear_prefix(db, EDGE_PREFIX)?;

    // Write concepts
    for (idx, concept) in concepts {
        let key = format!("{}{}", CONCEPT_PREFIX, idx.index());
        let val = bincode::serialize(concept).context("serializing concept")?;
        db.insert(key.as_bytes(), val)?;
    }

    // Write edges
    for (a, b, weight) in edges {
        let key = format!("{}{}:{}", EDGE_PREFIX, a.index(), b.index());
        let record = EdgeRecord {
            a: a.index(),
            b: b.index(),
            weight: *weight,
        };
        let val = bincode::serialize(&record).context("serializing edge")?;
        db.insert(key.as_bytes(), val)?;
    }

    db.flush()?;
    Ok(())
}

/// Rebuild graph data from sled entries.
/// Returns (concepts, edges).
#[allow(clippy::type_complexity)]
pub fn load_from_db(db: &Db) -> Result<(Vec<(usize, Concept)>, Vec<EdgeRecord>)> {
    let mut concepts = Vec::new();
    let mut edges = Vec::new();

    for item in db.iter() {
        let (key, val) = item.context("reading sled entry")?;
        let key_str = String::from_utf8_lossy(&key);

        if let Some(idx_str) = key_str.strip_prefix(CONCEPT_PREFIX) {
            let idx: usize = idx_str.parse().context("parsing node index")?;
            let concept: Concept = bincode::deserialize(&val).context("deserializing concept")?;
            concepts.push((idx, concept));
        } else if key_str.starts_with(EDGE_PREFIX) {
            let record: EdgeRecord = bincode::deserialize(&val).context("deserializing edge")?;
            edges.push(record);
        }
    }

    concepts.sort_by_key(|(idx, _)| *idx);
    Ok((concepts, edges))
}

fn clear_prefix(db: &Db, prefix: &str) -> Result<()> {
    let keys: Vec<Vec<u8>> = db
        .scan_prefix(prefix.as_bytes())
        .filter_map(|item| item.ok())
        .map(|(k, _)| k.to_vec())
        .collect();
    for k in keys {
        db.remove(&k)?;
    }
    Ok(())
}
