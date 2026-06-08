//! Detection d'activite vocale (VAD) sur PCM brut i16 : gate cheap -> modele cher.
pub mod vad;
pub use vad::{rms_norm, zero_crossing_rate, VadGate};
