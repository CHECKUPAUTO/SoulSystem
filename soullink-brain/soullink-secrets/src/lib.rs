//! Encrypted secret store with AES-256-GCM + HKDF-SHA256 per-secret key derivation.
//!
//! Ported from SoulSystem Integration's secrets module. Each secret has its own salt,
//! so identical plaintexts produce different ciphertexts.
//!
//! # Key Derivation
//! ```text
//! master_key ──┬──► HKDF-SHA256 ──► derived_key (per secret)
//!              │
//! salt ────────┘
//! ```

mod crypto;
mod store;
mod types;

pub use crypto::SecretsCrypto;
pub use store::SecretsStore;
pub use types::{DecryptedSecret, SecretError, SecretId, SecretMetadata, SecretValue};
