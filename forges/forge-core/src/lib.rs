#![forbid(unsafe_code)]
//! # forge-core
//!
//! Moteur de recherche evolutionnaire d'algorithmes **pilote par execution**.
//! Le LLM (ou une mutation) propose, le domaine compile/mesure/verifie sur le
//! terrain reel, le moteur selectionne et fait evoluer. La verite vient de
//! l'artefact execute, pas d'un raisonnement : c'est tout l'interet.
//!
//! ## Forme
//! - [`Domain`] : la frontiere d'extension. Une campagne = une implementation.
//!   Les 4 cibles (compression, quantification, kernels SIMD/GPU, routage MoE)
//!   sont 4 `Domain` independants ; le moteur n'en connait aucun.
//! - [`Engine`] : la boucle generique (seed -> evaluation -> archive -> mutation),
//!   avec rotation des entrees (anti-overfit) et validation holdout finale.
//! - Anti-triche : la porte de correction ([`Domain::verify`]) est separee de la
//!   mesure ([`Domain::measure`]) ; le candidat ne calcule jamais son score.
//!
//! La feature `llm` ajoute un client Ollama ([`llm::ollama_generate`]) pour le
//! generateur de candidats des domaines "code".

mod candidate;
mod domain;
mod error;
mod evolve;
pub mod isolation;
mod trial;

#[cfg(feature = "llm")]
pub mod llm;

pub use candidate::{fnv1a, Candidate, CandidateId};
pub use domain::{Domain, Score};
pub use error::{ForgeError, Result};
pub use evolve::{Checkpoint, Config, Engine, Individual, Report};
pub use isolation::run_with_timeout;
pub use trial::Trial;
