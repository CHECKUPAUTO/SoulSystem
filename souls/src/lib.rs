//! # `souls` — Runtime autonome de SoulSystem
//!
//! Ce crate expose la logique d'orchestration de l'entité autonome
//! (`SoulEntity`) + gateway + REPL, afin qu'elle puisse être invoquée
//! aussi bien par le binaire `souls` historique que par le binaire
//! unifié `soulsystem`.

pub mod config;
#[path = "runner.rs"]
pub mod runner;

pub use runner::{main_inner, Cli, Command};
