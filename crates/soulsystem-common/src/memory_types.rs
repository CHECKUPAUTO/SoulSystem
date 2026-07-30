//! Memory Types — Types partagés pour les résultats de recherche mémoire.

use serde::{Deserialize, Serialize};

/// Résultat de recherche mémoire (format unifié).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryHit {
    pub text: String,
    pub score: f32,
    pub source: String,
    #[serde(default)]
    pub timestamp: Option<String>,
}

/// Résultat de recherche vectorielle (format alternatif).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub content: String,
    pub score: f64,
    pub source: String,
}

impl From<MemoryHit> for SearchResult {
    fn from(hit: MemoryHit) -> Self {
        Self {
            content: hit.text,
            score: hit.score as f64,
            source: hit.source,
        }
    }
}

impl From<SearchResult> for MemoryHit {
    fn from(result: SearchResult) -> Self {
        Self {
            text: result.content,
            score: result.score as f32,
            source: result.source,
            timestamp: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hit_to_search_result() {
        let hit = MemoryHit {
            text: "hello".into(),
            score: 0.9,
            source: "test".into(),
            timestamp: None,
        };
        let sr: SearchResult = hit.into();
        assert_eq!(sr.content, "hello");
        assert!((sr.score - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_search_result_to_hit() {
        let sr = SearchResult {
            content: "world".into(),
            score: 0.75,
            source: "test".into(),
        };
        let hit: MemoryHit = sr.into();
        assert_eq!(hit.text, "world");
        assert!((hit.score - 0.75).abs() < 0.01);
    }
}

/// Trust carried *on the record itself*, so it survives wherever the record
/// goes — recall, promotion between layers, persistence, export.
///
/// This is the store's own vocabulary, deliberately smaller than the screening
/// pipeline's `TrustLevel`: quarantined content never reaches a store, so the
/// store has no word for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum MemoryTrust {
    /// Operator-supplied or system-internal; never crossed an untrusted
    /// boundary.
    Internal,
    /// Untrusted content that passed the injection scanner cleanly.
    Screened,
    /// Flagged by the scanner and fenced as inert data. Recall paths should
    /// not re-inject it as instruction-bearing text.
    Spotlighted,
    /// Model-generated (distillation, reflection). Derived from a context
    /// that may have contained untrusted material, so it inherits suspicion
    /// rather than authority.
    Derived,
    /// The write path recorded nothing. This is the deserialization default
    /// for pre-trust entries and the only honest label for a writer that
    /// did not say — an unrecorded write must not look clean.
    #[default]
    Unrecorded,
}

impl MemoryTrust {
    /// Where this level sits, higher = more trusted. Used only to take the
    /// floor when records merge.
    fn rank(self) -> u8 {
        match self {
            Self::Unrecorded => 0,
            Self::Spotlighted => 1,
            Self::Derived => 2,
            Self::Screened => 3,
            Self::Internal => 4,
        }
    }

    /// The least-trusted level among `levels`.
    ///
    /// A record produced by merging others is only as trustworthy as its
    /// least-trusted input: consolidation must not launder a Spotlighted
    /// member into a clean-looking semantic summary.
    pub fn floor_of(levels: impl IntoIterator<Item = MemoryTrust>) -> MemoryTrust {
        levels
            .into_iter()
            .min_by_key(|t| t.rank())
            .unwrap_or(MemoryTrust::Unrecorded)
    }
}

/// `is_instruction_bearing` — whether recall may re-inject this record's text
/// into a prompt as ordinary prose.
///
/// `Spotlighted` was flagged by the scanner and fenced once already;
/// re-injecting it as prose un-fences it. `Unrecorded` is content nobody
/// vouched for — pre-trust records and silent writers — and gets the same
/// caution: the cost of fencing clean-but-unlabelled text is a little prompt
/// noise, the cost of injecting a hostile unlabelled record is CRIT-005 again.
impl MemoryTrust {
    pub fn is_instruction_bearing(self) -> bool {
        matches!(self, Self::Internal | Self::Screened | Self::Derived)
    }
}

#[cfg(test)]
mod memory_trust_recall_tests {
    use super::MemoryTrust;

    /// MED-015-C's read-time contract: Spotlighted and Unrecorded content
    /// must not be re-injected into a prompt as ordinary prose.
    #[test]
    fn spotlighted_and_unrecorded_are_not_instruction_bearing() {
        assert!(!MemoryTrust::Spotlighted.is_instruction_bearing());
        assert!(
            !MemoryTrust::Unrecorded.is_instruction_bearing(),
            "content nobody vouched for gets fenced, not trusted by default"
        );
        assert!(MemoryTrust::Internal.is_instruction_bearing());
        assert!(MemoryTrust::Screened.is_instruction_bearing());
        assert!(MemoryTrust::Derived.is_instruction_bearing());
    }

    /// The serde default stays Unrecorded after the move to this crate — the
    /// re-export must not have changed pre-trust rows' honest reading.
    #[test]
    fn the_default_survived_the_move() {
        assert_eq!(MemoryTrust::default(), MemoryTrust::Unrecorded);
        let parsed: MemoryTrust = serde_json::from_str("\"Spotlighted\"").unwrap();
        assert_eq!(parsed, MemoryTrust::Spotlighted);
    }
}
