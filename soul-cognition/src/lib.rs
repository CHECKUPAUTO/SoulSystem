//! # soul-cognition
//!
//! Thin cognitive-coordination layer for the autonomous agent. It does **not**
//! re-implement memory, planning or the experimentation loop — those already
//! exist (`soullink-memory-hierarchy`, `soul_planner`, `soul_rsi`). It owns
//! only the connective tissue that enforces the four honesty invariants of the
//! autonomous-agent architecture (see `docs/COGNITIVE_ARCHITECTURE.md`):
//!
//! 1. Never fabricate observations.
//! 2. Never claim an unexecuted action as done.
//! 3. Always distinguish Observed / Deduced / Hypothetical.
//! 4. Level-2 (impactful) actions ALWAYS require confirmation.
//!
//! Invariants 1–3 are carried by [`provenance::Provenance`] / [`Tagged`];
//! invariant 4 is enforced by [`gate::PermissionGate`]. [`curiosity::Curiosity`]
//! supplies the one capability the existing crates lacked: intrinsic
//! motivation when no external goal is pending.
//!
//! [`curiosity::Curiosity`] supplies the one capability the existing crates
//! lacked: intrinsic motivation when no external goal is pending.
//!
//! The pieces compose in [`cognitive_loop::CognitiveLoop`], which wires
//! perception → recall → decide → experiment (`soul_rsi`) → consolidate →
//! reflect over the five-tier [`memory::CognitiveMemory`], gating every action.

pub mod cognitive_loop;
pub mod curiosity;
pub mod gate;
pub mod memory;
pub mod provenance;

pub use cognitive_loop::{CognitiveLoop, Focus};
pub use curiosity::{Curiosity, Probe, ScoredProbe};
pub use gate::{Confirmation, Decision, GateError, PermissionGate};
pub use memory::{CognitiveMemory, MemoryError, MemoryTier, Record};
pub use provenance::{Provenance, Tagged};

#[cfg(test)]
mod tests {
    use super::*;

    /// A small end-to-end check that the invariants compose: a hypothesis is
    /// never mistaken for an observation, and a destructive action proposed
    /// from that hypothesis is still gated.
    #[test]
    fn invariants_compose() {
        // (3) A measured fact is observed; an inference over it is deduced.
        let cpu = Tagged::observed("cpu=97%");
        assert!(cpu.provenance.is_observed());
        let diagnosis = Tagged::deduced("runaway process likely", &[cpu.provenance]);
        assert_eq!(diagnosis.provenance, Provenance::Deduced);
        assert!(!diagnosis.provenance.is_observed());

        // (4) Acting on that diagnosis with a destructive command is gated.
        let gate = PermissionGate::new();
        assert!(gate.authorize("reboot", None).is_err());
    }
}
