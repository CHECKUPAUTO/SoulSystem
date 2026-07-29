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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone)]
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
}

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
        }
    }

    /// Record one persistence decision.
    pub fn record(&mut self, provenance: MemoryProvenance) {
        self.latest
            .insert(Self::key(&provenance), provenance.clone());
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
