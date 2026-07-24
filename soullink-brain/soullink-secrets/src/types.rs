use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Unique identifier for a secret.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct SecretId(pub String);

impl std::fmt::Display for SecretId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A decrypted secret value.
///
/// `value` is [`SecretValue`], not a raw `Vec<u8>`: it zeroizes on drop and
/// its `Debug` impl is redacted, so a stray `{:?}` on a `DecryptedSecret`
/// (e.g. in a log statement) cannot print the plaintext (INV-SEC-1, INV-SEC-2).
#[derive(Debug)]
pub struct DecryptedSecret {
    pub id: SecretId,
    pub value: SecretValue,
    pub metadata: SecretMetadata,
}

/// Metadata about a stored secret.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretMetadata {
    pub created_at: u64,
    pub updated_at: u64,
    pub description: Option<String>,
    pub tags: Vec<String>,
}

/// Wrapper for secret values that prevents accidental logging and zeroizes
/// its contents on drop. Deliberately not `Clone`: a secret should have one
/// owner at a time, not silently-duplicated copies each needing their own
/// zeroization (INV-SEC-1).
pub struct SecretValue(Zeroizing<Vec<u8>>);

impl SecretValue {
    pub fn new(value: Vec<u8>) -> Self {
        Self(Zeroizing::new(value))
    }
    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretValue([REDACTED {} bytes])", self.0.len())
    }
}

/// Errors for secret operations.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("Invalid master key: must be at least 32 bytes")]
    InvalidMasterKey,
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),
    #[error("Secret not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_value_debug_redacts_bytes() {
        let secret = SecretValue::new(b"top-secret-api-key".to_vec());
        let debug = format!("{secret:?}");
        assert!(debug.contains("REDACTED"));
        assert!(!debug.contains("top-secret-api-key"));
        assert_eq!(secret.expose(), b"top-secret-api-key");
    }

    #[test]
    fn secret_value_debug_length_only_no_content() {
        // The redacted Debug output leaks the byte length (useful for
        // diagnostics) but never any byte of the content itself.
        let secret = SecretValue::new(vec![0xAB; 42]);
        let debug = format!("{secret:?}");
        assert_eq!(debug, "SecretValue([REDACTED 42 bytes])");
    }

    #[test]
    fn decrypted_secret_debug_does_not_leak_value() {
        let decrypted = DecryptedSecret {
            id: SecretId("api-key".into()),
            value: SecretValue::new(b"sk-super-secret-12345".to_vec()),
            metadata: SecretMetadata {
                created_at: 0,
                updated_at: 0,
                description: None,
                tags: vec![],
            },
        };
        let debug = format!("{decrypted:?}");
        assert!(
            !debug.contains("sk-super-secret-12345"),
            "DecryptedSecret Debug output must never contain the raw secret value, got: {debug}"
        );
        assert!(debug.contains("REDACTED"));
    }
}
