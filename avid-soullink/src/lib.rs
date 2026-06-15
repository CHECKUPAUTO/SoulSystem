#![forbid(unsafe_code)]
#![warn(unreachable_pub)]
#![allow(clippy::module_name_repetitions)]

pub mod agent;
pub mod archive;
pub mod blackboard;
pub mod intent;

pub use agent::{IntentExecutor, SoulLinkAgent, SoulLinkConfig, SoulLinkError};
pub use archive::{Archive, ArchiveError, MemoriArchive};
pub use blackboard::{Blackboard, BlackboardError, HttpBlackboard};
pub use intent::{Intent, IntentKind, IntentResult, OriginalityVerdict};
