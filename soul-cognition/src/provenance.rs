//! Observation provenance — the load-bearing mechanism behind the honesty
//! invariants "never fabricate observations" and "always distinguish
//! Observed / Deduced / Hypothetical".
//!
//! Every fact that flows through the cognitive loop carries a [`Provenance`]
//! tag. The key property is that provenance can only be *weakened* through
//! reasoning (an inference over observed facts is at best `Deduced`, never
//! `Observed`), and can only be *strengthened* by explicit verification — it
//! can never be silently upgraded.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Where a piece of information came from.
///
/// Ordered from strongest (most trustworthy) to weakest:
/// `Observed` > `Deduced` > `Hypothetical`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Provenance {
    /// Produced by an executed tool call or a measured metric. Ground truth.
    Observed,
    /// Derived by the agent from one or more `Observed` facts.
    Deduced,
    /// Proposed but not yet verified. Must never be presented as fact.
    Hypothetical,
}

impl Provenance {
    /// Combine the provenance of inputs feeding a derivation: the result is
    /// no stronger than its weakest input (the "weakest link" rule), and a
    /// derivation is itself at best `Deduced` even from purely observed
    /// inputs — reasoning never yields a fresh observation.
    pub fn derive_from(inputs: &[Provenance]) -> Provenance {
        let weakest = inputs.iter().copied().max().unwrap_or(Provenance::Observed);
        // A derivation can never be stronger than `Deduced`.
        weakest.max(Provenance::Deduced)
    }

    /// True only for ground-truth observations.
    pub fn is_observed(self) -> bool {
        self == Provenance::Observed
    }

    /// True when the information must not be stated as established fact.
    pub fn is_speculative(self) -> bool {
        self == Provenance::Hypothetical
    }

    /// Short marker used when rendering tagged content to a human/LLM.
    pub fn marker(self) -> &'static str {
        match self {
            Provenance::Observed => "[observed]",
            Provenance::Deduced => "[deduced]",
            Provenance::Hypothetical => "[hypothetical]",
        }
    }
}

impl fmt::Display for Provenance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Provenance::Observed => "observed",
            Provenance::Deduced => "deduced",
            Provenance::Hypothetical => "hypothetical",
        })
    }
}

/// A value carrying its provenance tag through the system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tagged<T> {
    pub value: T,
    pub provenance: Provenance,
}

impl<T> Tagged<T> {
    /// Construct an `Observed` fact. Reserve this for the perception boundary:
    /// only data that actually came back from an executed tool / measurement.
    pub fn observed(value: T) -> Self {
        Self {
            value,
            provenance: Provenance::Observed,
        }
    }

    /// Construct a `Hypothetical` value (a guess, a plan, an unverified claim).
    pub fn hypothetical(value: T) -> Self {
        Self {
            value,
            provenance: Provenance::Hypothetical,
        }
    }

    /// Construct a `Deduced` value with an explicit provenance derived from
    /// the inputs that produced it.
    pub fn deduced(value: T, inputs: &[Provenance]) -> Self {
        Self {
            value,
            provenance: Provenance::derive_from(inputs),
        }
    }

    /// Map the inner value, preserving the provenance tag.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Tagged<U> {
        Tagged {
            value: f(self.value),
            provenance: self.provenance,
        }
    }

    /// Render with the provenance marker prefixed — the safe way to surface a
    /// tagged value so a reader can never mistake a hypothesis for a fact.
    pub fn render(&self) -> String
    where
        T: fmt::Display,
    {
        format!("{} {}", self.provenance.marker(), self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordering_strongest_first() {
        assert!(Provenance::Observed < Provenance::Deduced);
        assert!(Provenance::Deduced < Provenance::Hypothetical);
    }

    #[test]
    fn derivation_is_never_observed() {
        // Even from purely observed inputs, a derivation is at best Deduced.
        let p = Provenance::derive_from(&[Provenance::Observed, Provenance::Observed]);
        assert_eq!(p, Provenance::Deduced);
    }

    #[test]
    fn derivation_takes_weakest_link() {
        let p = Provenance::derive_from(&[Provenance::Observed, Provenance::Hypothetical]);
        assert_eq!(p, Provenance::Hypothetical);
    }

    #[test]
    fn empty_derivation_defaults_to_deduced() {
        assert_eq!(Provenance::derive_from(&[]), Provenance::Deduced);
    }

    #[test]
    fn constructors_set_expected_tags() {
        assert!(Tagged::observed(1).provenance.is_observed());
        assert!(Tagged::hypothetical(1).provenance.is_speculative());
        let d = Tagged::deduced("x", &[Provenance::Observed]);
        assert_eq!(d.provenance, Provenance::Deduced);
    }

    #[test]
    fn render_prefixes_marker() {
        assert_eq!(Tagged::observed("cpu=42%").render(), "[observed] cpu=42%");
        assert_eq!(
            Tagged::hypothetical("maybe a leak").render(),
            "[hypothetical] maybe a leak"
        );
    }

    #[test]
    fn map_preserves_provenance() {
        let t = Tagged::hypothetical(2).map(|v| v * 10);
        assert_eq!(t.value, 20);
        assert_eq!(t.provenance, Provenance::Hypothetical);
    }
}
