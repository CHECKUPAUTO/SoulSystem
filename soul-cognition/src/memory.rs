//! Five-tier cognitive memory facade.
//!
//! The autonomous-agent architecture names five persistent memory types —
//! episodic, semantic, user, strategic, reflexive — plus a volatile working
//! buffer. Three of them (working/episodic/semantic) already exist as real,
//! consolidating stores in `soullink-memory-hierarchy`; this facade reuses
//! those unchanged and adds the two missing long-horizon tiers (strategic,
//! reflexive) and a typed `user` tier, so the whole set is addressable through
//! one provenance-aware API.
//!
//! Every record carries its [`Provenance`]. The facade enforces invariant #1
//! ("never fabricate observations"): a `Hypothetical` value cannot be written
//! into the long-term *fact* tiers (`Semantic`, `User`) — only established
//! knowledge (Observed/Deduced) may settle there. Episodic, reflexive and
//! working memory may hold speculation, because they record *what happened* or
//! *what was thought*, not *what is true*.

use crate::provenance::{Provenance, Tagged};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use soullink_memory_hierarchy::{
    ConsolidationConfig, EpisodicConfig, HierarchicalMemory, MemoryEntry, MemoryLayer,
    SemanticConfig,
};
use std::collections::HashMap;
use std::sync::RwLock;

/// Metadata key under which provenance is persisted in hierarchy entries.
const PROV_KEY: &str = "cognition.provenance";

/// The five persistent memory types plus the volatile working buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MemoryTier {
    /// Volatile active context (resets on restart).
    Working,
    /// Time-stamped events, fast decay.
    Episodic,
    /// Distilled long-term facts / concepts.
    Semantic,
    /// Stable facts about the operator/user.
    User,
    /// Long-horizon plans and lessons that shape future goals.
    Strategic,
    /// The agent's record of its own past behaviour (Reflexion signal).
    Reflexive,
}

impl MemoryTier {
    /// Long-term *fact* tiers that must not hold unverified speculation.
    fn is_fact_tier(self) -> bool {
        matches!(self, MemoryTier::Semantic | MemoryTier::User)
    }
}

/// A provenance-tagged memory record as seen through the facade.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub text: String,
    pub provenance: Provenance,
    pub importance: f32,
    pub created_at: String,
    pub tier: MemoryTier,
}

/// Why a memory operation was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MemoryError {
    /// Attempted to write a `Hypothetical` value into a long-term fact tier.
    #[error("cannot store hypothetical content in the {0:?} fact tier; verify it first")]
    SpeculativeNotAllowed(MemoryTier),
}

/// The unified five-tier memory. Cheap handles; clone freely.
pub struct CognitiveMemory {
    hierarchy: HierarchicalMemory,
    user: RwLock<Vec<Record>>,
    strategic: RwLock<Vec<Record>>,
    reflexive: RwLock<Vec<Record>>,
}

impl Default for CognitiveMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl CognitiveMemory {
    /// Construct with sensible defaults for an interactive agent.
    pub fn new() -> Self {
        Self::with_capacity(64)
    }

    /// Construct with a specific working-memory capacity.
    pub fn with_capacity(working_capacity: usize) -> Self {
        let hierarchy = HierarchicalMemory::new(
            working_capacity,
            EpisodicConfig::default(),
            SemanticConfig::default(),
            ConsolidationConfig::default(),
        );
        Self {
            hierarchy,
            user: RwLock::new(Vec::new()),
            strategic: RwLock::new(Vec::new()),
            reflexive: RwLock::new(Vec::new()),
        }
    }

    /// Store a tagged value into a tier. Enforces invariant #1 for fact tiers.
    pub async fn remember(
        &self,
        tier: MemoryTier,
        value: Tagged<String>,
        importance: f32,
    ) -> Result<(), MemoryError> {
        if tier.is_fact_tier() && value.provenance.is_speculative() {
            return Err(MemoryError::SpeculativeNotAllowed(tier));
        }
        let record = Record {
            text: value.value,
            provenance: value.provenance,
            importance,
            created_at: Utc::now().to_rfc3339(),
            tier,
        };
        match tier {
            MemoryTier::Working => {
                self.hierarchy
                    .store(self.entry(&record), MemoryLayer::Working)
                    .await;
            }
            MemoryTier::Episodic => {
                self.hierarchy
                    .store(self.entry(&record), MemoryLayer::Episodic)
                    .await;
            }
            MemoryTier::Semantic => {
                self.hierarchy
                    .store(self.entry(&record), MemoryLayer::Semantic)
                    .await;
            }
            MemoryTier::User => self.user.write().unwrap().push(record),
            MemoryTier::Strategic => self.strategic.write().unwrap().push(record),
            MemoryTier::Reflexive => self.reflexive.write().unwrap().push(record),
        }
        Ok(())
    }

    /// Recall the most relevant records across every tier, provenance preserved.
    pub async fn recall(&self, query: &str, limit: usize) -> Vec<Record> {
        let mut out: Vec<Record> = self
            .hierarchy
            .search(query, limit)
            .await
            .into_iter()
            .map(|e| self.record_from(e))
            .collect();

        for store in [&self.user, &self.strategic, &self.reflexive] {
            let guard = store.read().unwrap();
            let q = query.to_lowercase();
            out.extend(
                guard
                    .iter()
                    .filter(|r| r.text.to_lowercase().contains(&q))
                    .cloned(),
            );
        }

        out.sort_by(|a, b| {
            b.importance
                .partial_cmp(&a.importance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out.truncate(limit);
        out
    }

    /// Snapshot a single in-process tier (user/strategic/reflexive).
    pub fn snapshot(&self, tier: MemoryTier) -> Vec<Record> {
        match tier {
            MemoryTier::User => self.user.read().unwrap().clone(),
            MemoryTier::Strategic => self.strategic.read().unwrap().clone(),
            MemoryTier::Reflexive => self.reflexive.read().unwrap().clone(),
            // Hierarchy tiers are queried via `recall`, not snapshotted wholesale.
            _ => Vec::new(),
        }
    }

    /// Run a consolidation cycle (episodic → semantic). Returns how many
    /// clusters were promoted into semantic memory.
    pub async fn consolidate(&self) -> usize {
        self.hierarchy.consolidate().await.promoted
    }

    // ── internal conversions ────────────────────────────────────────────

    fn entry(&self, r: &Record) -> MemoryEntry {
        let mut metadata = HashMap::new();
        metadata.insert(
            PROV_KEY.to_string(),
            serde_json::Value::String(r.provenance.to_string()),
        );
        MemoryEntry {
            id: format!("{}-{}", r.created_at, fxhash(&r.text)),
            text: r.text.clone(),
            created_at: r.created_at.clone(),
            last_accessed: r.created_at.clone(),
            access_count: 0,
            importance: r.importance,
            layer: match r.tier {
                MemoryTier::Episodic => MemoryLayer::Episodic,
                MemoryTier::Semantic => MemoryLayer::Semantic,
                _ => MemoryLayer::Working,
            },
            tags: Vec::new(),
            embedding: None,
            associations: Vec::new(),
            metadata,
        }
    }

    fn record_from(&self, e: MemoryEntry) -> Record {
        // Default for foreign entries with no tag: `Deduced` — never silently
        // treat unknown provenance as ground-truth `Observed`.
        let provenance = e
            .metadata
            .get(PROV_KEY)
            .and_then(|v| v.as_str())
            .and_then(parse_provenance)
            .unwrap_or(Provenance::Deduced);
        let tier = match e.layer {
            MemoryLayer::Working => MemoryTier::Working,
            MemoryLayer::Episodic => MemoryTier::Episodic,
            MemoryLayer::Semantic => MemoryTier::Semantic,
        };
        Record {
            text: e.text,
            provenance,
            importance: e.importance,
            created_at: e.created_at,
            tier,
        }
    }
}

fn parse_provenance(s: &str) -> Option<Provenance> {
    match s {
        "observed" => Some(Provenance::Observed),
        "deduced" => Some(Provenance::Deduced),
        "hypothetical" => Some(Provenance::Hypothetical),
        _ => None,
    }
}

/// Tiny stable hash for entry ids (no external dep needed).
fn fxhash(s: &str) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fact_tiers_reject_hypothetical() {
        let mem = CognitiveMemory::new();
        let err = mem
            .remember(
                MemoryTier::Semantic,
                Tagged::hypothetical("the moon is cheese".into()),
                0.9,
            )
            .await
            .unwrap_err();
        assert_eq!(
            err,
            MemoryError::SpeculativeNotAllowed(MemoryTier::Semantic)
        );

        // User tier likewise refuses speculation.
        assert!(mem
            .remember(
                MemoryTier::User,
                Tagged::hypothetical("maybe likes tea".into()),
                0.5
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn fact_tiers_accept_established_knowledge() {
        let mem = CognitiveMemory::new();
        assert!(mem
            .remember(
                MemoryTier::Semantic,
                Tagged::observed("rust is memory safe".into()),
                0.9
            )
            .await
            .is_ok());
        assert!(mem
            .remember(
                MemoryTier::User,
                Tagged::deduced("prefers French".into(), &[Provenance::Observed]),
                0.8
            )
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn episodic_and_reflexive_allow_speculation() {
        let mem = CognitiveMemory::new();
        assert!(mem
            .remember(
                MemoryTier::Episodic,
                Tagged::hypothetical("might be a flaky test".into()),
                0.4
            )
            .await
            .is_ok());
        assert!(mem
            .remember(
                MemoryTier::Reflexive,
                Tagged::hypothetical("I may have over-pruned".into()),
                0.6
            )
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn recall_preserves_provenance_across_tiers() {
        let mem = CognitiveMemory::new();
        mem.remember(
            MemoryTier::User,
            Tagged::observed("zone is Europe/Paris".into()),
            0.9,
        )
        .await
        .unwrap();
        mem.remember(
            MemoryTier::Strategic,
            Tagged::deduced("focus on gateway parity".into(), &[Provenance::Observed]),
            0.95,
        )
        .await
        .unwrap();

        let zone = mem.recall("zone", 5).await;
        assert_eq!(zone.len(), 1);
        assert_eq!(zone[0].provenance, Provenance::Observed);
        assert_eq!(zone[0].tier, MemoryTier::User);

        let plan = mem.recall("gateway", 5).await;
        assert_eq!(plan[0].provenance, Provenance::Deduced);
        assert_eq!(plan[0].tier, MemoryTier::Strategic);
    }

    #[tokio::test]
    async fn snapshot_returns_in_process_tiers() {
        let mem = CognitiveMemory::new();
        mem.remember(
            MemoryTier::Reflexive,
            Tagged::observed("ran 3 steps".into()),
            0.5,
        )
        .await
        .unwrap();
        let snap = mem.snapshot(MemoryTier::Reflexive);
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].text, "ran 3 steps");
        // Hierarchy tiers are not snapshotted wholesale.
        assert!(mem.snapshot(MemoryTier::Episodic).is_empty());
    }

    #[tokio::test]
    async fn recall_ranks_by_importance() {
        let mem = CognitiveMemory::new();
        mem.remember(
            MemoryTier::Strategic,
            Tagged::observed("gateway low".into()),
            0.2,
        )
        .await
        .unwrap();
        mem.remember(
            MemoryTier::Strategic,
            Tagged::observed("gateway high".into()),
            0.9,
        )
        .await
        .unwrap();
        let r = mem.recall("gateway", 5).await;
        assert_eq!(r[0].text, "gateway high");
    }
}
