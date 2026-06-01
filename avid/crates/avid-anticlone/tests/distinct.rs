use avid_anticlone::{fingerprint_submission, similarity, Submission};
use std::collections::HashMap;

#[test]
fn distinct_code_is_low_similarity() {
    let files_a = HashMap::from([(
        "a.py".to_string(),
        "def factorial(n):\n    if n <= 1:\n        return 1\n    return n * factorial(n - 1)\n"
            .to_string(),
    )]);
    let sub_a = Submission {
        files: files_a,
        entrypoint: "a.py".to_string(),
    };

    let files_b = HashMap::from([(
        "b.py".to_string(),
        "class Counter:\n    def __init__(self):\n        self.val = 0\n    def inc(self):\n        self.val += 1\n"
            .to_string(),
    )]);
    let sub_b = Submission {
        files: files_b,
        entrypoint: "b.py".to_string(),
    };

    let fp_a = fingerprint_submission(&sub_a).expect("fingerprint a");
    let fp_b = fingerprint_submission(&sub_b).expect("fingerprint b");
    let score = similarity(&fp_a, &fp_b);
    assert!(score < 0.4, "expected distinct score < 0.4, got {score}");
}
