use avid_anticlone::{fingerprint_submission, similarity, Submission};
use std::collections::HashMap;

#[test]
fn identity_similarity_is_high() {
    let files = HashMap::from([(
        "main.py".to_string(),
        "def hello():\n    print('hello world')\n".to_string(),
    )]);
    let sub = Submission {
        files,
        entrypoint: "main.py".to_string(),
    };
    let fp1 = fingerprint_submission(&sub).expect("fingerprint");
    let fp2 = fingerprint_submission(&sub).expect("fingerprint");
    let score = similarity(&fp1, &fp2);
    // Same submission should have similarity very close to 1.0
    assert!(score > 0.99, "expected identity score > 0.99, got {score}");
}
