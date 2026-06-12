//! SoulLink Security — Secret scanner (TruffleHog-inspired, Rust-native).

pub mod detectors;
pub mod entropy;
pub mod patterns;
pub mod scanner;
pub mod verify;

pub use detectors::Finding;
pub use patterns::Severity;
pub use scanner::ScanResult;
pub use scanner::SecurityScanner;
