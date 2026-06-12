use avid_anticlone::{check, fingerprint_submission, Submission, Verdict};
use std::collections::HashMap;

#[test]
fn check_against_corpus_detects_clone() {
    let files = HashMap::from([(
        "main.py".to_string(),
        "def add(a, b):\n    return a + b\n".to_string(),
    )]);
    let sub = Submission {
        files,
        entrypoint: "main.py".to_string(),
    };
    let fp = fingerprint_submission(&sub).expect("fingerprint");

    let corpus = [(1, &fp)];
    let report = check(&fp, corpus, 0.60);
    assert!(
        matches!(report.verdict, Verdict::Clone),
        "expected Clone, got {:?}",
        report.verdict
    );
    assert_eq!(report.closest_id, Some(1));
    assert!(report.score > 0.99);
}

#[test]
fn check_against_empty_corpus_is_original() {
    let files = HashMap::from([(
        "main.py".to_string(),
        "def add(a, b):\n    return a + b\n".to_string(),
    )]);
    let sub = Submission {
        files,
        entrypoint: "main.py".to_string(),
    };
    let fp = fingerprint_submission(&sub).expect("fingerprint");

    let corpus: [(i64, &_); 0] = [];
    let report = check(&fp, corpus, 0.60);
    assert!(
        matches!(report.verdict, Verdict::Original),
        "expected Original, got {:?}",
        report.verdict
    );
    assert!(report.score < 0.01);
    assert_eq!(report.closest_id, None);
}
