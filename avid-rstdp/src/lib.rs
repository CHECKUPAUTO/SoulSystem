#![forbid(unsafe_code)]
#![warn(unreachable_pub)]
#![allow(clippy::module_name_repetitions)]

pub mod compiler;
pub mod feedback;
pub mod pattern;
pub mod store;

pub use compiler::{CompileEvent, CompilerError, HttpNcpuCompiler, NcpuCompiler};
pub use feedback::{FeedbackConfig, FeedbackLoop};
pub use pattern::{PatternHash, PatternSignature};
pub use store::{PatternStats, PatternStore, PatternStoreError};
