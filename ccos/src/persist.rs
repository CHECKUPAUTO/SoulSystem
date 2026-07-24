//! Kernel state persistence.
//!
//! A [`KernelSnapshot`] bundles the causal [`MemoryGraph`], the append-only
//! [`EventLog`] and the hash-chained [`DistributedEventLog`] into a single
//! self-describing JSON document so a session can be saved to disk and later
//! reloaded, verified or replayed (`ccos save` / `verify` / `replay`).

use crate::distributed_event_log::DistributedEventLog;
use crate::event_log::EventLog;
use crate::memory::MemoryGraph;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelSnapshot {
    /// Crate version that wrote the snapshot (for forward-compat checks).
    pub version: String,
    pub graph: MemoryGraph,
    pub event_log: EventLog,
    pub dist_log: DistributedEventLog,
}

impl KernelSnapshot {
    pub fn new(graph: MemoryGraph, event_log: EventLog, dist_log: DistributedEventLog) -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            graph,
            event_log,
            dist_log,
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Persist the snapshot to `path` as pretty JSON, durably and atomically
    /// (temp file + fsync + rename — see [`crate::util::write_durable`]): a
    /// crash mid-write leaves the prior good snapshot intact rather than a
    /// truncated one.
    pub fn save(&self, path: &str) -> std::io::Result<()> {
        let json = self
            .to_json()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        crate::util::write_durable(std::path::Path::new(path), json.as_bytes())
    }

    /// Load a snapshot previously written by [`KernelSnapshot::save`].
    pub fn load(path: &str) -> std::io::Result<Self> {
        let data = std::fs::read_to_string(path)?;
        Self::from_json(&data).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_log::{EventPayload, EventType};
    use crate::memory::NodeType;

    #[test]
    fn snapshot_json_roundtrip_preserves_state() {
        let mut graph = MemoryGraph::default();
        graph.upsert_node("a".into(), "A".into(), "x".into(), NodeType::Module);
        graph.upsert_node("b".into(), "B".into(), "y".into(), NodeType::Symbol);
        graph.add_edge(
            "a".into(),
            "b".into(),
            0.9,
            crate::memory::EdgeType::DependsOn,
        );

        let mut event_log = EventLog::new("persist-test".into());
        event_log.append(
            EventType::CycleStart,
            EventPayload::CycleEvent {
                cycle_number: 0,
                action: "init".into(),
            },
        );

        let mut dist_log = DistributedEventLog::new();
        dist_log.append("e0".into(), "kernel".into());

        let snap = KernelSnapshot::new(graph, event_log, dist_log);
        let json = snap.to_json().unwrap();
        let restored = KernelSnapshot::from_json(&json).unwrap();

        assert_eq!(restored.graph.node_count(), 2);
        assert_eq!(restored.graph.edge_count(), 1);
        assert_eq!(restored.event_log.event_count(), 1);
        assert!(restored.dist_log.verify_integrity().valid);
        assert_eq!(restored.version, env!("CARGO_PKG_VERSION"));
    }

    /// HIGH-005: KernelSnapshot::save must write via the atomic
    /// temp+fsync+rename path (`crate::util::write_durable`), not a bare
    /// `std::fs::write`. Asserts the wiring at the call site: save/load
    /// round-trips through a real file, no `.tmp` sibling is left behind,
    /// and a second save fully replaces the prior content.
    #[test]
    fn save_and_load_roundtrip_via_disk_leaves_no_temp_sibling() {
        let path = std::env::temp_dir().join(format!(
            "ccos-kernel-snapshot-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);
        let path_str = path.to_str().unwrap();

        let mut graph = MemoryGraph::default();
        graph.upsert_node("a".into(), "A".into(), "x".into(), NodeType::Module);
        let snap = KernelSnapshot::new(graph, EventLog::new("kernel-save".into()), {
            let mut d = DistributedEventLog::new();
            d.append("e0".into(), "kernel".into());
            d
        });
        snap.save(path_str).unwrap();

        let mut tmp = path.clone().into_os_string();
        tmp.push(".tmp");
        assert!(
            !std::path::Path::new(&tmp).exists(),
            "atomic save must not leave a .tmp sibling behind"
        );

        let loaded = KernelSnapshot::load(path_str).unwrap();
        assert_eq!(loaded.graph.node_count(), 1);

        // A second save with different content must fully replace the file.
        let mut bigger_graph = MemoryGraph::default();
        for i in 0..10 {
            bigger_graph.upsert_node(
                format!("n{i}").into(),
                format!("L{i}"),
                "content".into(),
                NodeType::Module,
            );
        }
        let snap2 = KernelSnapshot::new(
            bigger_graph,
            EventLog::new("kernel-save-2".into()),
            DistributedEventLog::new(),
        );
        snap2.save(path_str).unwrap();
        let reloaded = KernelSnapshot::load(path_str).unwrap();
        assert_eq!(reloaded.graph.node_count(), 10);

        let _ = std::fs::remove_file(&path);
    }
}
