#![forbid(unsafe_code)]
#![warn(clippy::pedantic, clippy::nursery)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]
#![allow(clippy::module_name_repetitions)]

//! Python AST fingerprinting and structural similarity engine.
//!
//! Parses Python source into ASTs via [`rustpython-parser`], extracts
//! structural fingerprints (node histograms, call-graph edges, depth
//! profiles), and computes originality scores via weighted Jaccard
//! similarity.
//!
//! # Example
//!
//! ```no_run
//! use avid_anticlone::{fingerprint_module, similarity};
//!
//! let fp1 = fingerprint_module("def f(): pass").expect("parse");
//! let fp2 = fingerprint_module("def g(): return 1").expect("parse");
//! let score = similarity(&fp1, &fp2);
//! assert!(score < 1.0);
//! ```

mod depth;
mod edges;
mod error;
mod fingerprint;
mod similarity;

use std::collections::HashMap;

pub use depth::{aggregate_depth, DepthProfile};
pub use error::AntiCloneError;
pub use fingerprint::{fingerprint_module, fingerprint_text, Fingerprint, NodeHist};
pub use similarity::{check, similarity, OriginalityReport, Verdict};

/// A code submission composed of named Python source files.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, garde::Validate)]
pub struct Submission {
    #[garde(skip)]
    pub files: HashMap<String, String>,
    #[garde(skip)]
    pub entrypoint: String,
}

/// Fingerprint a submission by aggregating fingerprints of each file.
///
/// # Errors
/// Returns `AntiCloneError` if any file fails to parse.
pub fn fingerprint_submission(submission: &Submission) -> Result<Fingerprint, AntiCloneError> {
    let mut agg = Fingerprint::default();
    for content in submission.files.values() {
        let fp = fingerprint_module(content)?;
        agg.merge(&fp);
    }
    Ok(agg)
}
