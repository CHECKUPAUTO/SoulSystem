#![forbid(unsafe_code)]
#![warn(clippy::pedantic, clippy::nursery)]
#![deny(clippy::todo, clippy::unimplemented)]
#![allow(
    clippy::module_name_repetitions,
    clippy::too_many_lines,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::cognitive_complexity
)]

//! Skill system for AVID — load, register, and execute SKILL.md-based agent skills.
//!
//! ## Overview
//!
//! The `avid-skills` crate provides a reusable skill system based on `SKILL.md` files —
//! a markdown format with YAML frontmatter that defines agent capabilities, allowed tools,
//! model preferences, and execution parameters.
//!
//! ## Architecture
//!
//! ```text
//! SKILL.md files → SkillRegistry → SkillExecutor
//!      │                │                │
//!   (on disk)     (in memory)     (calls LLM)
//! ```
//!
//! ## Usage
//!
//! ```rust,no_run
//! # use avid_skills::{SkillRegistry, SkillExecutor};
//! # use avid_core::llm::LlmClient;
//! # use std::path::Path;
//! #
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let mut registry = SkillRegistry::new();
//! registry.load_from_dir(Path::new("skills"))?;
//!
//! let executor = SkillExecutor::new(registry);
//! let client = LlmClient::new("http://localhost:11434".into(), "llama3:8b".into()).await?;
//!
//! let result = executor.execute("code-review", "fn main() {}", &client).await?;
//! println!("Score: {}", result.output);
//! # Ok(())
//! # }
//! ```

pub mod executor;
pub mod metadata;
pub mod registry;

pub use executor::{SkillExecutor, SkillResult};
pub use metadata::{Capability, Skill, SkillMetadata};
pub use registry::{spawn_hot_reload, SkillRegistry};
