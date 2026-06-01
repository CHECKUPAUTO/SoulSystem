use serde::{Deserialize, Serialize};

/// Unique identifier for a secret.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct SecretId(pub String);

impl std::fmt::Display for SecretId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A decrypted secret value.
#[derive(Debug, Clone)]
pub struct DecryptedSecret {
    pub id: SecretId,
    pub value: Vec<u8>,
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

/// Wrapper for secret values that prevents accidental logging.
#[derive(Clone)]
pub struct SecretValue(Vec<u8>);

impl SecretValue {
    pub fn new(value: Vec<u8>) -> Self {
        Self(value)
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
