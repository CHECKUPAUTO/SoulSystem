//! # `souls` — Runtime autonome de SoulSystem
//!
//! Ce crate expose la logique d'orchestration de l'entité autonome
//! (`SoulEntity`) + gateway + REPL, afin qu'elle puisse être invoquée
//! aussi bien par le binaire `souls` historique que par le binaire
//! unifié `soulsystem`.

#[path = "runner.rs"]
pub mod runner;

// config.rs is loaded as a module by runner.rs; re-export it here so the public
// path `souls::config` stays stable while the file is only loaded once (avoids
// clippy::duplicate_mod).
pub use runner::config;
pub use runner::{main_inner, Cli, Command};
