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
    /// Reconciliation key (Mem0-style). `None` for free-form records; `Some`
    /// for keyed facts managed via [`CognitiveMemory::reconcile_set`].
    #[serde(default)]
    pub key: Option<String>,
}

/// Why a memory operation was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MemoryError {
    /// Attempted to write a `Hypothetical` value into a long-term fact tier.
    #[error("cannot store hypothetical content in the {0:?} fact tier; verify it first")]
    SpeculativeNotAllowed(MemoryTier),
    /// A reconciliation would replace a stronger-provenance fact with a
    /// weaker-provenance value in a fact tier — refused so a guess never
    /// silently overwrites a known fact.
    #[error("refusing to overwrite a {existing} fact for '{key}' with a weaker {incoming} value")]
    WouldDowngrade {
        key: String,
        existing: Provenance,
        incoming: Provenance,
    },
    /// Reconciliation is only defined on the keyed in-process tiers
    /// (`User`, `Strategic`, `Reflexive`), not the consolidating hierarchy
    /// tiers (`Working`/`Episodic`/`Semantic`).
    #[error("reconciliation is not supported on the {0:?} tier")]
    ReconcileUnsupported(MemoryTier),
}

/// What a Mem0-style reconciliation decided for a keyed fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reconciliation {
    /// No fact existed for the key; the value was stored.
    Added,
    /// A fact existed and was replaced (value changed, or provenance
    /// strengthened for the same value). Carries the previous text.
    Updated { previous: String },
    /// A fact existed for the key and was removed. Carries the previous text.
    Deleted { previous: String },
    /// The incoming value matched what was already stored; nothing changed.
    NoOp,
}

/// Default cap on each in-process tier before importance-based eviction kicks
/// in. Bounds long-horizon growth (CraniMem-style gating).
const DEFAULT_TIER_CAPACITY: usize = 256;

/// The unified five-tier memory. Cheap handles; clone freely.
pub struct CognitiveMemory {
    hierarchy: HierarchicalMemory,
    user: RwLock<Vec<Record>>,
    strategic: RwLock<Vec<Record>>,
    reflexive: RwLock<Vec<Record>>,
    /// Max records per in-process tier; the lowest-importance (then oldest)
    /// record is evicted when a tier would exceed it.
    tier_capacity: usize,
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
            tier_capacity: DEFAULT_TIER_CAPACITY,
        }
    }

    /// Set the per-tier capacity for the in-process tiers (User/Strategic/
    /// Reflexive). When a tier would exceed it, the lowest-importance record
    /// (oldest among ties) is evicted — bounded memory for long-horizon runs.
    pub fn with_tier_capacity(mut self, capacity: usize) -> Self {
        self.tier_capacity = capacity.max(1);
        self
    }

    /// Number of records currently held in an in-process tier (0 for hierarchy
    /// tiers, which are queried via `recall`).
    pub fn tier_len(&self, tier: MemoryTier) -> usize {
        self.keyed_store(tier)
            .map(|s| s.read().unwrap().len())
            .unwrap_or(0)
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
            key: None,
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
            MemoryTier::User | MemoryTier::Strategic | MemoryTier::Reflexive => {
                let store = self.keyed_store(tier).expect("in-process tier");
                let mut guard = store.write().unwrap();
                guard.push(record);
                evict_to_capacity(&mut guard, self.tier_capacity);
            }
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

    /// Mem0-style reconcile of a **keyed** fact: instead of blindly appending,
    /// decide ADD / UPDATE / NOOP so the tier holds at most one current value
    /// per key.
    ///
    /// Honesty guarantees beyond Mem0:
    /// - a `Hypothetical` value is refused from a fact tier (invariant #1);
    /// - in a fact tier, a weaker-provenance value may **not** overwrite a
    ///   stronger established fact (`WouldDowngrade`) — a guess never silently
    ///   replaces a known fact;
    /// - re-asserting the same value with *stronger* provenance is an UPDATE
    ///   that strengthens the record (an honest "verified" event), not a NOOP.
    ///
    /// Only the keyed in-process tiers are supported; hierarchy tiers return
    /// [`MemoryError::ReconcileUnsupported`].
    pub fn reconcile_set(
        &self,
        tier: MemoryTier,
        key: &str,
        value: Tagged<String>,
        importance: f32,
    ) -> Result<Reconciliation, MemoryError> {
        let fact_tier = tier.is_fact_tier();
        if fact_tier && value.provenance.is_speculative() {
            return Err(MemoryError::SpeculativeNotAllowed(tier));
        }
        let store = self
            .keyed_store(tier)
            .ok_or(MemoryError::ReconcileUnsupported(tier))?;
        let mut guard = store.write().unwrap();

        if let Some(idx) = guard.iter().position(|r| r.key.as_deref() == Some(key)) {
            let existing_prov = guard[idx].provenance;
            let same_value = guard[idx].text == value.value;

            if same_value {
                // Same value: only act if the new provenance is *stronger*
                // (Observed < Deduced < Hypothetical in the ordering).
                if value.provenance < existing_prov {
                    let previous = guard[idx].text.clone();
                    guard[idx].provenance = value.provenance;
                    guard[idx].importance = importance;
                    guard[idx].created_at = Utc::now().to_rfc3339();
                    return Ok(Reconciliation::Updated { previous });
                }
                return Ok(Reconciliation::NoOp);
            }

            // Different value: in a fact tier, refuse a weaker overwrite.
            if fact_tier && value.provenance > existing_prov {
                return Err(MemoryError::WouldDowngrade {
                    key: key.to_string(),
                    existing: existing_prov,
                    incoming: value.provenance,
                });
            }
            let previous = guard[idx].text.clone();
            guard[idx] = Record {
                text: value.value,
                provenance: value.provenance,
                importance,
                created_at: Utc::now().to_rfc3339(),
                tier,
                key: Some(key.to_string()),
            };
            Ok(Reconciliation::Updated { previous })
        } else {
            guard.push(Record {
                text: value.value,
                provenance: value.provenance,
                importance,
                created_at: Utc::now().to_rfc3339(),
                tier,
                key: Some(key.to_string()),
            });
            evict_to_capacity(&mut guard, self.tier_capacity);
            Ok(Reconciliation::Added)
        }
    }

    /// Remove a keyed fact (Mem0 DELETE). Returns [`Reconciliation::Deleted`]
    /// with the prior text, or [`Reconciliation::NoOp`] if the key was absent.
    pub fn forget(&self, tier: MemoryTier, key: &str) -> Result<Reconciliation, MemoryError> {
        let store = self
            .keyed_store(tier)
            .ok_or(MemoryError::ReconcileUnsupported(tier))?;
        let mut guard = store.write().unwrap();
        if let Some(idx) = guard.iter().position(|r| r.key.as_deref() == Some(key)) {
            let previous = guard.remove(idx).text;
            Ok(Reconciliation::Deleted { previous })
        } else {
            Ok(Reconciliation::NoOp)
        }
    }

    /// The current value of a keyed fact, if present.
    pub fn fact(&self, tier: MemoryTier, key: &str) -> Option<Record> {
        let store = self.keyed_store(tier)?;
        let guard = store.read().unwrap();
        guard
            .iter()
            .find(|r| r.key.as_deref() == Some(key))
            .cloned()
    }

    /// The `RwLock<Vec<Record>>` backing a keyed in-process tier, if any.
    fn keyed_store(&self, tier: MemoryTier) -> Option<&RwLock<Vec<Record>>> {
        match tier {
            MemoryTier::User => Some(&self.user),
            MemoryTier::Strategic => Some(&self.strategic),
            MemoryTier::Reflexive => Some(&self.reflexive),
            _ => None,
        }
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
            key: None,
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

/// Evict the lowest-priority records until `records` fits within `cap`.
/// Priority is `(importance, recency)`: the lowest-importance record is removed
/// first, breaking ties by evicting the oldest (`created_at` sorts
/// chronologically as RFC-3339).
fn evict_to_capacity(records: &mut Vec<Record>, cap: usize) {
    while records.len() > cap {
        let victim = records
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                a.importance
                    .partial_cmp(&b.importance)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.created_at.cmp(&b.created_at))
            })
            .map(|(i, _)| i);
        match victim {
            Some(i) => {
                records.remove(i);
            }
            None => break,
        }
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

    #[test]
    fn reconcile_add_then_noop_then_update() {
        let mem = CognitiveMemory::new();
        // First assertion of a key → Added.
        assert_eq!(
            mem.reconcile_set(
                MemoryTier::User,
                "timezone",
                Tagged::observed("Europe/Paris".into()),
                0.9
            )
            .unwrap(),
            Reconciliation::Added
        );
        // Same value, same provenance → NoOp.
        assert_eq!(
            mem.reconcile_set(
                MemoryTier::User,
                "timezone",
                Tagged::observed("Europe/Paris".into()),
                0.9
            )
            .unwrap(),
            Reconciliation::NoOp
        );
        // New observed value → Updated, carries the previous.
        assert_eq!(
            mem.reconcile_set(
                MemoryTier::User,
                "timezone",
                Tagged::observed("Europe/London".into()),
                0.9
            )
            .unwrap(),
            Reconciliation::Updated {
                previous: "Europe/Paris".into()
            }
        );
        // Only one record for the key, the current value.
        let f = mem.fact(MemoryTier::User, "timezone").unwrap();
        assert_eq!(f.text, "Europe/London");
        assert_eq!(mem.snapshot(MemoryTier::User).len(), 1);
    }

    #[test]
    fn reconcile_refuses_weaker_overwrite_in_fact_tier() {
        let mem = CognitiveMemory::new();
        mem.reconcile_set(
            MemoryTier::User,
            "language",
            Tagged::observed("French".into()),
            0.9,
        )
        .unwrap();
        // A weaker (Deduced) value cannot overwrite the Observed fact.
        let err = mem
            .reconcile_set(
                MemoryTier::User,
                "language",
                Tagged::deduced("German".into(), &[Provenance::Deduced]),
                0.5,
            )
            .unwrap_err();
        assert!(matches!(err, MemoryError::WouldDowngrade { .. }));
        // The known fact is untouched.
        assert_eq!(
            mem.fact(MemoryTier::User, "language").unwrap().text,
            "French"
        );
    }

    #[test]
    fn reconcile_strengthens_provenance_on_reassert() {
        let mem = CognitiveMemory::new();
        // Strategic is not a fact tier, so a Deduced value is allowed.
        mem.reconcile_set(
            MemoryTier::Strategic,
            "focus",
            Tagged::deduced("ship gateway".into(), &[Provenance::Deduced]),
            0.8,
        )
        .unwrap();
        // Re-asserting the SAME value with stronger (Observed) provenance is a
        // verification → Updated, and the stored provenance strengthens.
        assert_eq!(
            mem.reconcile_set(
                MemoryTier::Strategic,
                "focus",
                Tagged::observed("ship gateway".into()),
                0.9
            )
            .unwrap(),
            Reconciliation::Updated {
                previous: "ship gateway".into()
            }
        );
        assert_eq!(
            mem.fact(MemoryTier::Strategic, "focus").unwrap().provenance,
            Provenance::Observed
        );
    }

    #[test]
    fn reconcile_rejects_hypothetical_in_fact_tier() {
        let mem = CognitiveMemory::new();
        let err = mem
            .reconcile_set(
                MemoryTier::User,
                "city",
                Tagged::hypothetical("maybe Berlin".into()),
                0.5,
            )
            .unwrap_err();
        assert_eq!(err, MemoryError::SpeculativeNotAllowed(MemoryTier::User));
    }

    #[test]
    fn forget_deletes_then_noops() {
        let mem = CognitiveMemory::new();
        mem.reconcile_set(
            MemoryTier::User,
            "timezone",
            Tagged::observed("Europe/Paris".into()),
            0.9,
        )
        .unwrap();
        assert_eq!(
            mem.forget(MemoryTier::User, "timezone").unwrap(),
            Reconciliation::Deleted {
                previous: "Europe/Paris".into()
            }
        );
        // Second forget is a NoOp.
        assert_eq!(
            mem.forget(MemoryTier::User, "timezone").unwrap(),
            Reconciliation::NoOp
        );
        assert!(mem.fact(MemoryTier::User, "timezone").is_none());
    }

    #[tokio::test]
    async fn tier_capacity_evicts_lowest_importance() {
        let mem = CognitiveMemory::new().with_tier_capacity(2);
        for (text, imp) in [("keep-high", 0.9f32), ("drop-low", 0.1), ("keep-mid", 0.5)] {
            mem.remember(MemoryTier::Strategic, Tagged::observed(text.into()), imp)
                .await
                .unwrap();
        }
        // Over capacity by one → the lowest-importance record was evicted.
        assert_eq!(mem.tier_len(MemoryTier::Strategic), 2);
        let texts: Vec<String> = mem
            .snapshot(MemoryTier::Strategic)
            .into_iter()
            .map(|r| r.text)
            .collect();
        assert!(texts.contains(&"keep-high".to_string()));
        assert!(texts.contains(&"keep-mid".to_string()));
        assert!(!texts.contains(&"drop-low".to_string()));
    }

    #[test]
    fn reconcile_add_respects_capacity() {
        let mem = CognitiveMemory::new().with_tier_capacity(2);
        for (key, imp) in [("a", 0.9f32), ("b", 0.1), ("c", 0.5)] {
            mem.reconcile_set(
                MemoryTier::Strategic,
                key,
                Tagged::observed(format!("val-{key}")),
                imp,
            )
            .unwrap();
        }
        assert_eq!(mem.tier_len(MemoryTier::Strategic), 2);
        // The lowest-importance keyed fact ('b') was evicted.
        assert!(mem.fact(MemoryTier::Strategic, "b").is_none());
        assert!(mem.fact(MemoryTier::Strategic, "a").is_some());
        assert!(mem.fact(MemoryTier::Strategic, "c").is_some());
    }

    #[test]
    fn reconcile_unsupported_on_hierarchy_tiers() {
        let mem = CognitiveMemory::new();
        let err = mem
            .reconcile_set(MemoryTier::Semantic, "k", Tagged::observed("v".into()), 0.5)
            .unwrap_err();
        assert_eq!(err, MemoryError::ReconcileUnsupported(MemoryTier::Semantic));
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
