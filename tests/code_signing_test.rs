use sha2::{Digest, Sha256};
use soulsystem::code_signing::*;

#[test]
fn test_verify_valid_code() {
    let code = "fn main() { println!(\"ok\"); }";
    let pubkey = vec![42u8; 32];
    let digest = Sha256::digest(code.as_bytes());
    let signature: Vec<u8> = digest
        .iter()
        .zip(pubkey.iter().cycle())
        .map(|(a, b)| a ^ b)
        .collect();

    let signed = SignedCode {
        code: code.into(),
        signature,
        public_key: pubkey.clone(),
    };

    let mut auth = AuthorizedKeys::load().unwrap_or_else(|_| AuthorizedKeys {
        keys: std::collections::HashSet::new(),
    });
    auth.add_key(&pubkey).unwrap();
    verify_code(&signed, &auth).expect("Valid code should verify");
}

#[test]
fn test_reject_invalid_key() {
    let signed = SignedCode {
        code: "fn main() {}".into(),
        signature: vec![0u8; 32],
        public_key: vec![99u8; 32],
    };
    let auth = AuthorizedKeys::load().unwrap_or_else(|_| AuthorizedKeys {
        keys: std::collections::HashSet::new(),
    });
    assert!(verify_code(&signed, &auth).is_err());
}
