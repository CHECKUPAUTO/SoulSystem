//! Guard for MED-004: no fabricated embeddings feeding semantic scores.
//!
//! `soul-agent-core` built `vec![1.0_f32; 64]` for both the query and the
//! thought and scored them against each other. Cosine similarity of two
//! identical constant vectors is 1.0 regardless of the text they claim to
//! represent, so **every** node in the Tree of Thoughts scored a perfect 1.0:
//! nothing was ever pruned, `best_path` chose arbitrarily among equals, and
//! the tree reported confident semantic numbers it had never computed.
//!
//! There are two independent defences now, and this guard covers the outer
//! one. `ThoughtTree::evaluate_node` refuses constant embeddings at the API
//! boundary and marks such nodes `Unscored` — that is enforced by unit tests
//! in soullink-reasoning. This file enforces the thing a type cannot: that no
//! caller goes back to *manufacturing* an embedding to get past it.

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} unreadable: {e}", path.display()))
}

fn code_lines(source: &str) -> Vec<&str> {
    source
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("//") && !l.starts_with("///") && !l.starts_with('*'))
        .collect()
}

/// No constant-vector embedding is constructed in agent code.
///
/// Matches the shape `vec![<literal>; N]` bound to a name containing "emb",
/// which is what a fabricated embedding looks like. A real embedding comes
/// from a provider call, never from a repeat-expression.
#[test]
fn no_constant_vector_is_built_as_an_embedding() {
    let source = read("soul-agent-core/src/lib.rs");
    let offenders: Vec<&str> = code_lines(&source)
        .into_iter()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("emb") && lower.contains("vec![") && lower.contains(';')
        })
        .collect();

    assert!(
        offenders.is_empty(),
        "an embedding is being constructed as a constant vector:\n  {}\n\nA \
         constant vector scores 1.0 against any other constant vector, so \
         every node is accepted and pruning becomes a no-op while the scores \
         still read as semantic judgements. Use a real embedding provider, or \
         call ThoughtTree::mark_unscored to record that no judgement was \
         possible (MED-004).",
        offenders.join("\n  ")
    );
}

/// The refusal in `evaluate_node` is still present.
///
/// The unit tests in soullink-reasoning prove it *works*; this proves it has
/// not been deleted as an inconvenience by someone who wanted the old
/// placeholder path back.
#[test]
fn evaluate_node_still_refuses_uninformative_embeddings() {
    let source = read("soullink-brain/soullink-reasoning/src/tree.rs");

    assert!(
        source.contains("fn embedding_is_informative"),
        "ThoughtTree lost its embedding_is_informative check; evaluate_node \
         would once again score constant placeholder vectors"
    );
    assert!(
        source.contains("NodeStatus::Unscored"),
        "evaluate_node no longer marks unscoreable nodes Unscored"
    );
}

/// `Unscored` exists and is documented as distinct from Pending and Pruned.
#[test]
fn the_unscored_status_is_still_distinct() {
    let source = read("soullink-brain/soullink-reasoning/src/node.rs");
    assert!(
        source.contains("Unscored"),
        "NodeStatus::Unscored was removed. Folding it into Pending or Pruned \
         loses the distinction between \"not looked at\", \"judged poor\" and \
         \"no judgement was possible\"."
    );
}

/// The guard reads real files rather than passing on empty reads.
#[test]
fn the_guard_reads_the_files_it_claims_to() {
    for rel in [
        "soul-agent-core/src/lib.rs",
        "soullink-brain/soullink-reasoning/src/tree.rs",
        "soullink-brain/soullink-reasoning/src/node.rs",
    ] {
        assert!(
            read(rel).len() > 500,
            "{rel} is suspiciously small; the guard would pass vacuously"
        );
    }
}
