//! Provenance and trust metadata for persisted memory records (INV-MEM-3).
//!
//! Screening (see [`crate::screening`]) decides whether untrusted content may
//! be persisted at all. This module records **what happened to it** — where it
//! came from, how far it was trusted, and which store it landed in — so that a
//! record recalled hours later can still be told apart from one the operator
//! typed in themselves.
//!
//! # Why the trust level has to travel with the bytes
//!
//! Before this module, [`crate::screening::screen`] returned the verdict
//! *alongside* the content, and every persist call site dropped it on the
//! floor. Spotlight-fenced suspicious content and genuinely clean content were
//! byte-strings of the same type, stored identically, and recalled into the
//! next prompt with nothing to tell them apart. The fence survived; the
//! knowledge that a fence had been needed did not.
//!
//! So [`TrustLevel`] is carried *on* `ScreenedContent` rather than passed
//! beside it. A call site cannot persist the content while forgetting the
//! trust, because it never holds one without the other.
//!
//! # What this is not
//!
//! This is bookkeeping, not enforcement. It records that a record came from a
//! tool named `read_file` because that is what the dispatcher said; it cannot
//! verify the claim. Enforcement lives in the type system — the private
//! `ScreenedContent` constructor and the private store fields on
//! `AutonomousAgent` — and provenance describes what that enforcement allowed
//! through.
//!
//! Provenance is also **in-process and bounded**: [`ProvenanceLog`] keeps a
//! ring of the most recent entries and a latest-per-URI index. It is not a
//! durable audit log, and a record evicted from the ring leaves its store
//! entry behind with no provenance to look up. That is a deliberate limit of
//! this pass, stated rather than papered over — durable provenance that
//! survives restart belongs with the store, and the stores here (CCOS's causal
//! graph, the planner's action ring, OctaSoma) have no field for it.

use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// How far the system trusts a piece of content.
///
/// Ordered from most to least trusted. The distinction that matters at a
/// persist site is [`TrustLevel::is_persistable`]: quarantined content has had
/// its payload withheld, and what remains is a placeholder describing an
/// attack — never something to write into a content store.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum TrustLevel {
    /// Operator-supplied or system-internal. Never crossed an untrusted
    /// boundary, so it was never screened — trusted by construction, not by
    /// inspection.
    Trusted,
    /// Untrusted content that passed the injection scanner cleanly. This is
    /// the *scanner's* verdict, not proof of safety: a payload the scanner has
    /// no signature for lands here.
    Screened,
    /// The scanner found it suspicious. The content was spotlight-fenced as
    /// inert data before it got here — usable, but it should not be treated as
    /// instruction-bearing text, and a recall path that re-injects it into a
    /// prompt is re-injecting something already flagged once.
    Spotlighted,
    /// The scanner found it malicious. The raw payload was **withheld** — the
    /// content at this level is a placeholder, not the original bytes.
    Quarantined,
}

impl TrustLevel {
    /// Whether content at this trust level may be written to a durable content
    /// store.
    ///
    /// Only [`TrustLevel::Quarantined`] is refused. Writing the quarantine
    /// placeholder into a content store is not a safe fallback: it is a
    /// *destructive* one, because the placeholder displaces whatever the store
    /// held for that record before. See `ccos_observe_tool`'s handling and the
    /// `quarantined_read_does_not_erase_*` regression tests.
    pub fn is_persistable(self) -> bool {
        !matches!(self, TrustLevel::Quarantined)
    }

    /// Short stable label for logs, events and provenance dumps.
    pub fn label(self) -> &'static str {
        match self {
            TrustLevel::Trusted => "trusted",
            TrustLevel::Screened => "screened",
            TrustLevel::Spotlighted => "spotlighted",
            TrustLevel::Quarantined => "quarantined",
        }
    }
}

/// Where a memory record originally came from.
///
/// This is about *origin*, not about trust: a `ToolOutput` that screened clean
/// and a `ModelOutput` that screened clean both carry [`TrustLevel::Screened`],
/// but only the first came from outside the process.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MemorySource {
    /// Output of a named tool — untrusted external data by definition.
    ToolOutput {
        /// The dispatched tool's name, as the dispatcher reported it.
        tool: String,
    },
    /// The model's own assistant message.
    ///
    /// Screened all the same. The model reads tool output and then writes its
    /// own conclusion; an injected instruction that the model repeats in its
    /// own words is laundered through a source that *looks* internal. Treating
    /// model output as trusted-by-origin would let exactly that through.
    ModelOutput,
    /// Operator input or system-internal bookkeeping.
    System,
}

impl MemorySource {
    /// Short stable label for logs and provenance dumps.
    pub fn label(&self) -> String {
        match self {
            MemorySource::ToolOutput { tool } => format!("tool:{tool}"),
            MemorySource::ModelOutput => "model".to_string(),
            MemorySource::System => "system".to_string(),
        }
    }

    /// Whether this origin is outside the process boundary and therefore must
    /// be screened before it may be persisted.
    pub fn is_untrusted(&self) -> bool {
        matches!(
            self,
            MemorySource::ToolOutput { .. } | MemorySource::ModelOutput
        )
    }
}

/// Which durable store a record was written to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MemoryStore {
    /// CCOS causal graph (`ingest_source` / `signal_failure`).
    CausalGraph,
    /// Planner action history ring.
    PlannerHistory,
    /// OctaSoma topical semantic memory.
    Semantic,
}

impl MemoryStore {
    /// Short stable label for logs and provenance dumps.
    pub fn label(self) -> &'static str {
        match self {
            MemoryStore::CausalGraph => "causal",
            MemoryStore::PlannerHistory => "planner",
            MemoryStore::Semantic => "semantic",
        }
    }
}

/// What is known about one persisted memory record.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MemoryProvenance {
    /// Where the content came from.
    pub source: MemorySource,
    /// How far it was trusted at the moment it was persisted.
    pub trust: TrustLevel,
    /// Which store it was written to.
    pub store: MemoryStore,
    /// Store-specific locator — `file:src/main.rs` for the causal graph, the
    /// tool invocation for planner history, a content digest for semantic
    /// memory. Not globally unique across stores.
    pub uri: String,
    /// The injection scanner's score at screening time. `0` for
    /// [`TrustLevel::Trusted`] content, which is never scanned.
    pub screening_score: u32,
    /// Whether the write was actually performed. `false` means the record was
    /// **refused** — quarantined content that would have displaced a real
    /// entry. The provenance entry is kept either way, so a refusal is
    /// visible rather than silent.
    pub persisted: bool,
    /// When the decision was made.
    pub recorded_at: DateTime<Utc>,
}

impl MemoryProvenance {
    /// One-line rendering for operator-facing output.
    pub fn summary(&self) -> String {
        format!(
            "{} {} {} [{}]{}",
            self.store.label(),
            self.uri,
            self.trust.label(),
            self.source.label(),
            if self.persisted { "" } else { " REFUSED" },
        )
    }
}

/// Default number of provenance entries retained.
///
/// The log is a debugging and operator-visibility aid, not an audit trail; it
/// is bounded so a long autonomous run cannot grow it without limit.
pub const DEFAULT_PROVENANCE_CAPACITY: usize = 512;

/// A bounded ring of [`MemoryProvenance`] entries plus a latest-per-URI index.
///
/// The index answers "what do we know about this record *now*", the ring
/// answers "what happened recently". A URI's index entry outlives its ring
/// entry: re-ingesting the same file repeatedly should not cost a
/// linear scan to answer the first question.
#[derive(Debug)]
pub struct ProvenanceLog {
    entries: std::collections::VecDeque<MemoryProvenance>,
    latest: HashMap<String, MemoryProvenance>,
    capacity: usize,
    /// Keys whose full detail was dropped from `latest` at [`INDEX_CAPACITY`],
    /// with the trust level retained.
    ///
    /// Trust is the field a caller actually decides on; the rest of a
    /// `MemoryProvenance` is explanatory. Keeping a `TrustLevel` per key costs
    /// one byte and preserves the only distinction that changes behaviour, so
    /// this is what survives when the detail cannot.
    demoted: HashMap<String, TrustLevel>,
    /// Insertion order of `latest` keys, for evicting the oldest.
    index_order: std::collections::VecDeque<String>,
    /// Keys dropped from `demoted` as well — genuinely forgotten.
    ///
    /// A count rather than the keys: retaining the keys is what the cap exists
    /// to avoid. It exists so "we have forgotten things" is *visible*, since
    /// past this point [`ProvenanceLookup::Unknown`] becomes ambiguous again.
    forgotten: usize,
    /// Where the durable index lives, if this log was opened against a path.
    path: Option<std::path::PathBuf>,
}

/// Maximum keys retained in the latest-per-record index.
///
/// The ring was already bounded; the index was **not**, so the stated bound did
/// not bound anything — a long run re-ingesting distinct URIs grew `latest`
/// without limit. The bound is much larger than the ring because an entry here
/// is one key and one `TrustLevel`, not a full record.
pub const INDEX_CAPACITY: usize = 65_536;

/// What the log knows about one record.
///
/// The three cases exist because `Option` conflated two of them. A caller that
/// gets `None` from a lookup cannot tell "this content was screened and the
/// note aged out" from "this content was never screened at all" — and those
/// call for opposite decisions. INV-MEM-3 exists to stop everything looking
/// equally trusted; a lookup that answers `None` for a record whose trust is
/// known is that same failure in miniature.
#[derive(Debug, Clone, PartialEq)]
pub enum ProvenanceLookup<'a> {
    /// Full provenance is retained: origin, trust, score, and whether the
    /// write was allowed.
    Known(&'a MemoryProvenance),
    /// The record was screened and recorded, but its detail has been evicted.
    /// The trust level survived, which is the part a caller acts on.
    TrustOnly(TrustLevel),
    /// Nothing is known. Either the record predates provenance tracking, was
    /// written through a path that does not record it, or was forgotten
    /// entirely — see [`ProvenanceLog::forgotten_count`].
    Unknown,
}

impl ProvenanceLookup<'_> {
    /// The trust level, when one is known at all.
    pub fn trust(&self) -> Option<TrustLevel> {
        match self {
            Self::Known(p) => Some(p.trust),
            Self::TrustOnly(t) => Some(*t),
            Self::Unknown => None,
        }
    }

    /// Whether anything at all is known about this record.
    ///
    /// Note the asymmetry this makes explicit: `false` is *not* evidence the
    /// content is safe. It is the absence of evidence either way.
    pub fn is_known(&self) -> bool {
        !matches!(self, Self::Unknown)
    }
}

/// The on-disk form of the index.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct DurableIndex {
    /// Bumped when the shape changes so an old file is refused rather than
    /// misread.
    format_version: u32,
    entries: Vec<MemoryProvenance>,
    demoted: Vec<(String, TrustLevel)>,
    forgotten: usize,
}

const DURABLE_FORMAT_VERSION: u32 = 1;

impl ProvenanceLog {
    /// A log retaining `capacity` recent entries. A `capacity` of zero is
    /// raised to one — a log that silently discards everything written to it
    /// would report "no provenance" indistinguishably from a record that never
    /// existed.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: std::collections::VecDeque::new(),
            latest: HashMap::new(),
            capacity: capacity.max(1),
            demoted: HashMap::new(),
            index_order: std::collections::VecDeque::new(),
            forgotten: 0,
            path: None,
        }
    }

    /// Record one persistence decision.
    pub fn record(&mut self, provenance: MemoryProvenance) {
        let key = Self::key(&provenance);
        // A re-record of the same key must not queue the key twice, or the
        // order deque would evict a key that is still live in `latest`.
        if self
            .latest
            .insert(key.clone(), provenance.clone())
            .is_none()
        {
            self.index_order.push_back(key.clone());
        }
        // Re-recording a key that had been demoted promotes it back to full
        // detail; leaving the stale demotion would shadow the fresh entry.
        self.demoted.remove(&key);

        while self.index_order.len() > INDEX_CAPACITY {
            if let Some(old) = self.index_order.pop_front() {
                if let Some(p) = self.latest.remove(&old) {
                    self.demoted.insert(old, p.trust);
                }
            }
        }
        // `demoted` is bounded too, or the leak would just move house. Past
        // this point a key is genuinely forgotten, which `forgotten` records.
        while self.demoted.len() > INDEX_CAPACITY {
            if let Some(k) = self.demoted.keys().next().cloned() {
                self.demoted.remove(&k);
                self.forgotten += 1;
            } else {
                break;
            }
        }

        self.entries.push_back(provenance);
        while self.entries.len() > self.capacity {
            self.entries.pop_front();
        }
    }

    /// The latest decision recorded for `uri` in `store`, if the log has one.
    ///
    /// `None` means *nothing is known* — either the record was never written
    /// through a provenance-recording path, or its entry was evicted. It does
    /// not mean the record is absent from the store.
    pub fn latest_for(&self, store: MemoryStore, uri: &str) -> Option<&MemoryProvenance> {
        self.latest.get(&format!("{}\u{0}{}", store.label(), uri))
    }

    /// What is known about `uri` in `store`, distinguishing "evicted" from
    /// "never recorded".
    ///
    /// Prefer this to [`Self::latest_for`] at any site that *decides*
    /// something. `latest_for` answers `None` in both cases, and treating a
    /// record whose trust is known as if it were unscreened is the conflation
    /// INV-MEM-3 exists to prevent.
    pub fn lookup(&self, store: MemoryStore, uri: &str) -> ProvenanceLookup<'_> {
        let key = format!("{}\u{0}{}", store.label(), uri);
        if let Some(p) = self.latest.get(&key) {
            return ProvenanceLookup::Known(p);
        }
        if let Some(t) = self.demoted.get(&key) {
            return ProvenanceLookup::TrustOnly(*t);
        }
        ProvenanceLookup::Unknown
    }

    /// How many records have been forgotten entirely.
    ///
    /// Non-zero means [`ProvenanceLookup::Unknown`] is no longer conclusive:
    /// some records that *were* screened now answer `Unknown`. Surfacing the
    /// count is the difference between a bounded structure and a lossy one that
    /// looks complete.
    pub fn forgotten_count(&self) -> usize {
        self.forgotten
    }

    /// How many records retain a trust level but no longer their full detail.
    pub fn demoted_count(&self) -> usize {
        self.demoted.len()
    }

    /// Recent entries, oldest first.
    pub fn recent(&self) -> impl Iterator<Item = &MemoryProvenance> {
        self.entries.iter()
    }

    /// How many entries the ring currently holds.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the ring is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many recent entries record a **refused** write.
    pub fn refused_count(&self) -> usize {
        self.entries.iter().filter(|e| !e.persisted).count()
    }

    fn key(p: &MemoryProvenance) -> String {
        format!("{}\u{0}{}", p.store.label(), p.uri)
    }

    // ── Durability ────────────────────────────────────────────────────────

    /// Open a log backed by `path`, loading any index already there.
    ///
    /// A missing file is an empty log, not an error: first run is the common
    /// case and must not require a setup step.
    ///
    /// A file that is present but **unreadable or malformed** is a different
    /// matter, and this returns an error rather than silently starting empty.
    /// Starting empty would turn "the provenance store is corrupt" into "every
    /// record is unscreened" — the exact state INV-MEM-3 exists to prevent,
    /// arrived at by way of a failure nobody saw.
    pub fn open(path: impl Into<std::path::PathBuf>, capacity: usize) -> std::io::Result<Self> {
        let path = path.into();
        let mut log = Self::with_capacity(capacity);
        log.path = Some(path.clone());

        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(log),
            Err(e) => return Err(e),
        };

        let index: DurableIndex = serde_json::from_str(&raw).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("provenance index at {} is unreadable: {e}", path.display()),
            )
        })?;
        if index.format_version != DURABLE_FORMAT_VERSION {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "provenance index at {} is format version {}, expected {}",
                    path.display(),
                    index.format_version,
                    DURABLE_FORMAT_VERSION
                ),
            ));
        }

        for p in index.entries {
            let key = Self::key(&p);
            if log.latest.insert(key.clone(), p).is_none() {
                log.index_order.push_back(key);
            }
        }
        log.demoted = index.demoted.into_iter().collect();
        log.forgotten = index.forgotten;
        Ok(log)
    }

    /// Write the index to the path this log was opened against.
    ///
    /// Temp-file-plus-rename, for the same reason the CCOS state directory uses
    /// it (INV-PERSIST-1): a crash midway through must leave the previous index
    /// intact rather than a truncated one. A half-written index is worse than a
    /// stale one — it deserializes as fewer known records, so content that was
    /// screened comes back `Unknown`.
    ///
    /// The **ring is not persisted**, only the index. The ring answers "what
    /// happened recently", which is a debugging aid scoped to a process; the
    /// index answers "what is this record", which is what has to outlive one.
    pub fn persist(&self) -> std::io::Result<()> {
        let Some(path) = self.path.as_ref() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "provenance log has no path; construct it with ProvenanceLog::open",
            ));
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let index = DurableIndex {
            format_version: DURABLE_FORMAT_VERSION,
            entries: self.latest.values().cloned().collect(),
            demoted: self.demoted.iter().map(|(k, v)| (k.clone(), *v)).collect(),
            forgotten: self.forgotten,
        };
        let json = serde_json::to_string(&index)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// The path backing this log, if any.
    pub fn path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
    }
}

impl Default for ProvenanceLog {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_PROVENANCE_CAPACITY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prov(store: MemoryStore, uri: &str, trust: TrustLevel, persisted: bool) -> MemoryProvenance {
        MemoryProvenance {
            source: MemorySource::ToolOutput {
                tool: "read_file".to_string(),
            },
            trust,
            store,
            uri: uri.to_string(),
            screening_score: 0,
            persisted,
            recorded_at: Utc::now(),
        }
    }

    #[test]
    fn only_quarantined_content_is_refused_persistence() {
        assert!(TrustLevel::Trusted.is_persistable());
        assert!(TrustLevel::Screened.is_persistable());
        assert!(
            TrustLevel::Spotlighted.is_persistable(),
            "spotlight-fencing is the mitigation; dropping suspicious content \
             entirely would lose real data to false positives"
        );
        assert!(!TrustLevel::Quarantined.is_persistable());
    }

    #[test]
    fn trust_levels_order_from_most_to_least_trusted() {
        assert!(TrustLevel::Trusted < TrustLevel::Screened);
        assert!(TrustLevel::Screened < TrustLevel::Spotlighted);
        assert!(TrustLevel::Spotlighted < TrustLevel::Quarantined);
    }

    #[test]
    fn model_output_counts_as_untrusted_origin() {
        // The laundering path: the model reads an injected tool result and
        // repeats the instruction in its own words. If model output were
        // trusted by origin, that would persist unscreened.
        assert!(MemorySource::ModelOutput.is_untrusted());
        assert!(MemorySource::ToolOutput {
            tool: "read_file".into()
        }
        .is_untrusted());
        assert!(!MemorySource::System.is_untrusted());
    }

    #[test]
    fn latest_for_returns_the_most_recent_decision_per_record() {
        let mut log = ProvenanceLog::default();
        log.record(prov(
            MemoryStore::CausalGraph,
            "file:a.rs",
            TrustLevel::Screened,
            true,
        ));
        log.record(prov(
            MemoryStore::CausalGraph,
            "file:a.rs",
            TrustLevel::Quarantined,
            false,
        ));

        let latest = log
            .latest_for(MemoryStore::CausalGraph, "file:a.rs")
            .expect("recorded");
        assert_eq!(latest.trust, TrustLevel::Quarantined);
        assert!(!latest.persisted);
        assert_eq!(log.len(), 2, "both decisions stay in the ring");
    }

    #[test]
    fn the_same_uri_in_two_stores_does_not_collide() {
        let mut log = ProvenanceLog::default();
        log.record(prov(
            MemoryStore::CausalGraph,
            "x",
            TrustLevel::Screened,
            true,
        ));
        log.record(prov(
            MemoryStore::Semantic,
            "x",
            TrustLevel::Quarantined,
            false,
        ));

        assert_eq!(
            log.latest_for(MemoryStore::CausalGraph, "x")
                .expect("causal")
                .trust,
            TrustLevel::Screened,
        );
        assert_eq!(
            log.latest_for(MemoryStore::Semantic, "x")
                .expect("semantic")
                .trust,
            TrustLevel::Quarantined,
        );
    }

    #[test]
    fn the_ring_is_bounded_but_the_index_survives_eviction() {
        let mut log = ProvenanceLog::with_capacity(2);
        for i in 0..5 {
            log.record(prov(
                MemoryStore::CausalGraph,
                &format!("file:{i}.rs"),
                TrustLevel::Screened,
                true,
            ));
        }
        assert_eq!(log.len(), 2, "ring bounded");
        assert!(
            log.latest_for(MemoryStore::CausalGraph, "file:0.rs")
                .is_some(),
            "the per-URI index answers 'what is known now' even after the \
             ring entry aged out"
        );
    }

    #[test]
    fn zero_capacity_is_raised_to_one_rather_than_discarding_silently() {
        let mut log = ProvenanceLog::with_capacity(0);
        log.record(prov(
            MemoryStore::CausalGraph,
            "file:a.rs",
            TrustLevel::Screened,
            true,
        ));
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn refused_writes_are_counted_not_dropped() {
        let mut log = ProvenanceLog::default();
        log.record(prov(
            MemoryStore::CausalGraph,
            "file:a.rs",
            TrustLevel::Screened,
            true,
        ));
        log.record(prov(
            MemoryStore::CausalGraph,
            "file:b.rs",
            TrustLevel::Quarantined,
            false,
        ));
        assert_eq!(log.refused_count(), 1);
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn summary_marks_a_refusal_visibly() {
        let refused = prov(
            MemoryStore::CausalGraph,
            "file:a.rs",
            TrustLevel::Quarantined,
            false,
        );
        let summary = refused.summary();
        assert!(summary.contains("REFUSED"), "got {summary}");
        assert!(summary.contains("quarantined"), "got {summary}");
        assert!(summary.contains("tool:read_file"), "got {summary}");
    }
}

#[cfg(test)]
mod durability_tests {
    use super::*;

    fn prov(uri: &str, trust: TrustLevel) -> MemoryProvenance {
        MemoryProvenance {
            source: MemorySource::ToolOutput {
                tool: "read_file".into(),
            },
            trust,
            store: MemoryStore::CausalGraph,
            uri: uri.into(),
            screening_score: 0,
            persisted: true,
            recorded_at: chrono::Utc::now(),
        }
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("soul_provenance_tests");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir.join(format!("{name}.json"))
    }

    /// P1-6-B acceptance test 1: a trust level survives a process restart.
    ///
    /// "Restart" is modelled as dropping the log entirely and opening a fresh
    /// one from the same path — the same thing the process does, without
    /// needing a process.
    #[test]
    fn a_trust_level_survives_a_restart() {
        let path = temp_path("restart");
        let _ = std::fs::remove_file(&path);

        {
            let mut log = ProvenanceLog::open(&path, 8).expect("open");
            log.record(prov("file:src/a.rs", TrustLevel::Spotlighted));
            log.record(prov("file:src/b.rs", TrustLevel::Screened));
            log.persist().expect("persist");
        }

        let reopened = ProvenanceLog::open(&path, 8).expect("reopen");
        assert_eq!(
            reopened
                .lookup(MemoryStore::CausalGraph, "file:src/a.rs")
                .trust(),
            Some(TrustLevel::Spotlighted),
            "a spotlighted record must not come back looking clean"
        );
        assert_eq!(
            reopened
                .lookup(MemoryStore::CausalGraph, "file:src/b.rs")
                .trust(),
            Some(TrustLevel::Screened)
        );
        let _ = std::fs::remove_file(&path);
    }

    /// P1-6-B acceptance test 2: a record whose provenance was evicted is
    /// distinguishable from one that was never screened.
    ///
    /// Driven through the real eviction path rather than by reaching into the
    /// struct, so it tests what `record` actually does at the cap.
    #[test]
    fn an_evicted_record_is_distinguishable_from_an_unscreened_one() {
        let mut log = ProvenanceLog::with_capacity(4);
        log.record(prov("file:evicted.rs", TrustLevel::Spotlighted));

        // Push the key out of the detail index.
        for i in 0..INDEX_CAPACITY {
            log.record(prov(&format!("file:filler{i}.rs"), TrustLevel::Screened));
        }

        let evicted = log.lookup(MemoryStore::CausalGraph, "file:evicted.rs");
        let never = log.lookup(MemoryStore::CausalGraph, "file:never-seen.rs");

        assert_ne!(evicted, never, "the two cases must not be the same answer");
        assert_eq!(
            evicted,
            ProvenanceLookup::TrustOnly(TrustLevel::Spotlighted),
            "an evicted record must keep the trust level a caller decides on"
        );
        assert_eq!(never, ProvenanceLookup::Unknown);
        assert!(evicted.is_known());
        assert!(!never.is_known());
    }

    /// The bug this fixes: the ring was bounded but the index was not, so the
    /// documented bound bounded nothing.
    #[test]
    fn the_index_is_bounded_not_just_the_ring() {
        let mut log = ProvenanceLog::with_capacity(4);
        for i in 0..(INDEX_CAPACITY + 500) {
            log.record(prov(&format!("file:{i}.rs"), TrustLevel::Screened));
        }
        assert_eq!(log.len(), 4, "the ring keeps its own bound");
        assert!(
            log.latest.len() <= INDEX_CAPACITY,
            "index grew to {} past the {INDEX_CAPACITY} cap",
            log.latest.len()
        );
        assert!(
            log.demoted_count() > 0,
            "keys pushed out of the index should be demoted, not dropped"
        );
    }

    /// Re-recording a key must not queue it twice, or the order deque would
    /// evict a key that is still live.
    #[test]
    fn re_recording_a_key_does_not_double_queue_it() {
        let mut log = ProvenanceLog::with_capacity(4);
        for _ in 0..10 {
            log.record(prov("file:same.rs", TrustLevel::Screened));
        }
        assert_eq!(log.latest.len(), 1);
        assert_eq!(log.index_order.len(), 1);
    }

    /// A demoted key that is written again must come back at full detail, not
    /// stay shadowed by the stale demotion.
    #[test]
    fn re_recording_a_demoted_key_promotes_it_back() {
        let mut log = ProvenanceLog::with_capacity(4);
        log.record(prov("file:target.rs", TrustLevel::Spotlighted));
        for i in 0..INDEX_CAPACITY {
            log.record(prov(&format!("file:f{i}.rs"), TrustLevel::Screened));
        }
        assert!(matches!(
            log.lookup(MemoryStore::CausalGraph, "file:target.rs"),
            ProvenanceLookup::TrustOnly(_)
        ));

        log.record(prov("file:target.rs", TrustLevel::Screened));
        match log.lookup(MemoryStore::CausalGraph, "file:target.rs") {
            ProvenanceLookup::Known(p) => assert_eq!(p.trust, TrustLevel::Screened),
            other => panic!("expected Known after re-record, got {other:?}"),
        }
    }

    /// A corrupt index must fail loudly. Starting empty would turn "the store
    /// is broken" into "nothing was ever screened".
    #[test]
    fn a_corrupt_index_is_an_error_not_an_empty_log() {
        let path = temp_path("corrupt");
        std::fs::write(&path, "{ this is not json").expect("write");
        let err = ProvenanceLog::open(&path, 8).expect_err("must refuse");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        let _ = std::fs::remove_file(&path);
    }

    /// A future format must be refused rather than misread.
    #[test]
    fn a_future_format_version_is_refused() {
        let path = temp_path("version");
        std::fs::write(
            &path,
            r#"{"format_version":999,"entries":[],"demoted":[],"forgotten":0}"#,
        )
        .expect("write");
        let err = ProvenanceLog::open(&path, 8).expect_err("must refuse");
        assert!(format!("{err}").contains("999"), "{err}");
        let _ = std::fs::remove_file(&path);
    }

    /// A missing file is first run, not an error.
    #[test]
    fn a_missing_index_opens_empty() {
        let path = temp_path("absent");
        let _ = std::fs::remove_file(&path);
        let log = ProvenanceLog::open(&path, 8).expect("open");
        assert!(log.is_empty());
        assert_eq!(
            log.lookup(MemoryStore::CausalGraph, "file:x.rs"),
            ProvenanceLookup::Unknown
        );
    }

    /// Persisting without a path is a caller error, not a silent no-op.
    #[test]
    fn persisting_a_pathless_log_is_an_error() {
        let log = ProvenanceLog::with_capacity(4);
        assert!(log.persist().is_err());
    }

    /// The forgotten counter must make an inconclusive `Unknown` visible.
    #[test]
    fn forgotten_records_are_counted() {
        let mut log = ProvenanceLog::with_capacity(2);
        assert_eq!(log.forgotten_count(), 0);
        for i in 0..(INDEX_CAPACITY * 2 + 10) {
            log.record(prov(&format!("file:{i}.rs"), TrustLevel::Screened));
        }
        assert!(
            log.forgotten_count() > 0,
            "past both caps, records are genuinely forgotten and that must be visible"
        );
    }

    /// A refused (quarantined) write must survive a restart as a refusal.
    #[test]
    fn a_refusal_survives_a_restart() {
        let path = temp_path("refusal");
        let _ = std::fs::remove_file(&path);
        {
            let mut log = ProvenanceLog::open(&path, 8).expect("open");
            let mut p = prov("file:bad.rs", TrustLevel::Quarantined);
            p.persisted = false;
            log.record(p);
            log.persist().expect("persist");
        }
        let reopened = ProvenanceLog::open(&path, 8).expect("reopen");
        match reopened.lookup(MemoryStore::CausalGraph, "file:bad.rs") {
            ProvenanceLookup::Known(p) => {
                assert!(!p.persisted, "the refusal must not come back as a write");
                assert_eq!(p.trust, TrustLevel::Quarantined);
            }
            other => panic!("expected Known, got {other:?}"),
        }
        let _ = std::fs::remove_file(&path);
    }
}

#[cfg(test)]
mod backup_qualification_tests {
    use super::*;

    fn prov(uri: &str, trust: TrustLevel) -> MemoryProvenance {
        MemoryProvenance {
            source: MemorySource::ToolOutput {
                tool: "read_file".into(),
            },
            trust,
            store: MemoryStore::CausalGraph,
            uri: uri.into(),
            screening_score: 0,
            persisted: true,
            recorded_at: chrono::Utc::now(),
        }
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("soul_provenance_backup_tests");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    /// P1-7-C qualification: a file-copy backup of the index, restored,
    /// answers exactly what the index answered when the backup was taken —
    /// including *not* knowing what was recorded afterwards. A backup is a
    /// point in time; a restore that answered `Known` for a record made
    /// after the backup would be fabricating provenance.
    #[test]
    fn a_restored_backup_answers_as_of_the_backup_not_as_of_now() {
        let live = temp_path("live.json");
        let backup = temp_path("backup.json");
        let _ = std::fs::remove_file(&live);
        let _ = std::fs::remove_file(&backup);

        let mut log = ProvenanceLog::open(&live, 64).unwrap();
        log.record(prov("file:a.rs", TrustLevel::Screened));
        log.record(prov("file:b.rs", TrustLevel::Spotlighted));
        log.persist().unwrap();

        std::fs::copy(&live, &backup).unwrap();

        log.record(prov("file:c.rs", TrustLevel::Screened));
        log.persist().unwrap();

        let restored = ProvenanceLog::open(&backup, 64).unwrap();
        assert!(matches!(
            restored.lookup(MemoryStore::CausalGraph, "file:a.rs"),
            ProvenanceLookup::Known(p) if p.trust == TrustLevel::Screened
        ));
        assert!(matches!(
            restored.lookup(MemoryStore::CausalGraph, "file:b.rs"),
            ProvenanceLookup::Known(p) if p.trust == TrustLevel::Spotlighted
        ));
        assert!(
            matches!(
                restored.lookup(MemoryStore::CausalGraph, "file:c.rs"),
                ProvenanceLookup::Unknown
            ),
            "a record made after the backup must be Unknown in the restore — \
             anything else fabricates provenance"
        );

        // And the live index still knows all three.
        let live_log = ProvenanceLog::open(&live, 64).unwrap();
        assert!(matches!(
            live_log.lookup(MemoryStore::CausalGraph, "file:c.rs"),
            ProvenanceLookup::Known(_)
        ));

        let _ = std::fs::remove_file(&live);
        let _ = std::fs::remove_file(&backup);
    }

    /// A corrupt backup must refuse to open, not restore as an empty index.
    /// An empty index and a corrupt one give the same `Unknown` answers, but
    /// only one of them is telling the truth — the failure has to be loud at
    /// restore time, when an operator can still go find a better copy.
    #[test]
    fn a_corrupt_backup_is_refused_not_restored_as_empty() {
        let backup = temp_path("corrupt-backup.json");
        std::fs::write(&backup, b"{ this is not an index").unwrap();

        let err = ProvenanceLog::open(&backup, 64);
        assert!(
            err.is_err(),
            "a corrupt backup opening as an empty index would silently \
             answer Unknown for everything and look healthy doing it"
        );

        let _ = std::fs::remove_file(&backup);
    }
}
