use soulsystem::code_signing::*;

#[cfg(feature = "ed25519")]
use ed25519_dalek::{Signer, SigningKey};

#[cfg(not(feature = "ed25519"))]
use sha2::{Digest, Sha256};

fn make_auth_with_key(key: &[u8]) -> AuthorizedKeys {
    let mut auth = AuthorizedKeys::load().unwrap_or_else(|_| AuthorizedKeys {
        keys: std::collections::HashSet::new(),
    });
    auth.add_key(key).unwrap();
    auth
}

#[test]
fn test_verify_valid_code() {
    let code = "fn main() { println!(\"ok\"); }";

    #[cfg(feature = "ed25519")]
    {
        let signing_key = SigningKey::from_bytes(&[42u8; 32]);
        let verifying_key = signing_key.verifying_key();
        let signature = signing_key.sign(code.as_bytes());

        let signed = SignedCode {
            code: code.into(),
            signature: signature.to_bytes().to_vec(),
            public_key: verifying_key.to_bytes().to_vec(),
        };

        let auth = make_auth_with_key(&verifying_key.to_bytes());
        verify_code(&signed, &auth).expect("Valid code should verify");
    }

    #[cfg(not(feature = "ed25519"))]
    {
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

        let auth = make_auth_with_key(&pubkey);
        verify_code(&signed, &auth).expect("Valid code should verify");
    }
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
