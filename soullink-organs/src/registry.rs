//! Organ registry — tracks organ identities and ports in the mesh.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrganId {
    Science = 0,
    Mind = 1,
    Engineer = 2,
    Crypto = 3,
    Creative = 4,
    Meta = 5,
    Memory = 6,
    Reflex = 7,
    Integration = 8,
    Perception = 9,
    Affect = 10,
    Language = 11,
    Reasoning = 12,
}

impl OrganId {
    pub const ALL: &[OrganId] = &[
        Self::Science,
        Self::Mind,
        Self::Engineer,
        Self::Crypto,
        Self::Creative,
        Self::Meta,
        Self::Memory,
        Self::Reflex,
        Self::Integration,
        Self::Perception,
        Self::Affect,
        Self::Language,
        Self::Reasoning,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Self::Science => "science",
            Self::Mind => "mind",
            Self::Engineer => "engineer",
            Self::Crypto => "crypto",
            Self::Creative => "creative",
            Self::Meta => "meta",
            Self::Memory => "memory",
            Self::Reflex => "reflex",
            Self::Integration => "integration",
            Self::Perception => "perception",
            Self::Affect => "affect",
            Self::Language => "language",
            Self::Reasoning => "reasoning",
        }
    }

    pub fn port(&self) -> u16 {
        match self {
            Self::Science => 9010,
            Self::Mind => 9011,
            Self::Engineer => 9012,
            Self::Crypto => 9013,
            Self::Creative => 9014,
            Self::Meta => 9015,
            Self::Memory => 9030,
            Self::Reflex => 9035,
            Self::Integration => 9032,
            Self::Perception => 9033,
            Self::Affect => 9034,
            Self::Language => 9036,
            Self::Reasoning => 9031,
        }
    }
}

#[derive(Debug, Default)]
pub struct OrganRegistry {}

impl OrganRegistry {
    pub fn new() -> Self {
        Self::default()
    }
}
