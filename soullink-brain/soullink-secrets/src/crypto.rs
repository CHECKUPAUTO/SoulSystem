//! AES-256-GCM encryption with per-secret HKDF-SHA256 key derivation.
//! Ported from IronClaw secrets/crypto.rs.
//!
//! `master_key` and every key/plaintext derived from it are held in
//! zeroizing wrappers (INV-SEC-1, see `docs/security/SECURITY_INVARIANTS.md`):
//! process memory never retains a scrubbable copy after the wrapper is
//! dropped, so a core dump or `/proc/pid/mem` read taken after use cannot
//! recover them.

use aes_gcm::{
    aead::{Aead, AeadCore, OsRng},
    Aes256Gcm, KeyInit, Nonce,
};
use hkdf::Hkdf;
use secrecy::{ExposeSecret, Secret};
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::types::SecretError;

const KEY_SIZE: usize = 32;
const NONCE_SIZE: usize = 12;
const SALT_SIZE: usize = 32;
const INFO: &[u8] = b"soullink-secret-v1";

/// Cryptographic engine for secret encryption.
///
/// `master_key` is a `secrecy::Secret`, which zeroizes its contents on drop
/// and is never printed by a derived `Debug` impl (this struct intentionally
/// has none). It cannot be cloned, so there is exactly one copy of the master
/// key in memory for the lifetime of a `SecretsCrypto`.
pub struct SecretsCrypto {
    master_key: Secret<Vec<u8>>,
}

impl SecretsCrypto {
    pub fn new(master_key: &[u8]) -> Result<Self, SecretError> {
        if master_key.len() < KEY_SIZE {
            return Err(SecretError::InvalidMasterKey);
        }
        Ok(Self {
            master_key: Secret::new(master_key.to_vec()),
        })
    }

    /// Generate a random salt.
    pub fn generate_salt() -> Vec<u8> {
        let mut salt = vec![0u8; SALT_SIZE];
        rand::RngCore::fill_bytes(&mut OsRng, &mut salt);
        salt
    }

    /// Derive a per-secret key using HKDF-SHA256. The result is a distinct
    /// secret in its own right (usable to decrypt data encrypted with it), so
    /// it is zeroized on drop like the master key.
    fn derive_key(&self, salt: &[u8]) -> Result<Zeroizing<Vec<u8>>, SecretError> {
        let hkdf = Hkdf::<Sha256>::new(Some(salt), self.master_key.expose_secret());
        let mut derived = Zeroizing::new(vec![0u8; KEY_SIZE]);
        hkdf.expand(INFO, &mut derived)
            .map_err(|e| SecretError::EncryptionFailed(format!("HKDF expand failed: {}", e)))?;
        Ok(derived)
    }

    /// Encrypt plaintext. Returns (nonce+ciphertext+tag, salt).
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), SecretError> {
        let salt = Self::generate_salt();
        let derived_key = self.derive_key(&salt)?;

        let cipher = Aes256Gcm::new_from_slice(&derived_key)
            .map_err(|e| SecretError::EncryptionFailed(format!("Cipher init failed: {}", e)))?;

        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(&nonce, plaintext)
            .map_err(|e| SecretError::EncryptionFailed(format!("Encrypt failed: {}", e)))?;

        // Pack: nonce || ciphertext (ciphertext includes auth tag)
        let mut packed = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
        packed.extend_from_slice(&nonce);
        packed.extend_from_slice(&ciphertext);

        Ok((packed, salt))
    }

    /// Decrypt packed ciphertext with salt. Returns the plaintext secret,
    /// zeroized on drop.
    pub fn decrypt(&self, packed: &[u8], salt: &[u8]) -> Result<Zeroizing<Vec<u8>>, SecretError> {
        let derived_key = self.derive_key(salt)?;

        let cipher = Aes256Gcm::new_from_slice(&derived_key)
            .map_err(|e| SecretError::DecryptionFailed(format!("Cipher init failed: {}", e)))?;

        if packed.len() < NONCE_SIZE {
            return Err(SecretError::DecryptionFailed(
                "Packed data too short".into(),
            ));
        }

        let nonce = Nonce::from_slice(&packed[..NONCE_SIZE]);
        let ciphertext = &packed[NONCE_SIZE..];

        cipher
            .decrypt(nonce, ciphertext)
            .map(Zeroizing::new)
            .map_err(|e| SecretError::DecryptionFailed(format!("Decrypt failed: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroize::Zeroize;

    /// Safe, non-UB proof that the zeroization mechanism actually clears
    /// bytes (not merely that the value is wrapped in a type that claims to).
    /// Reading memory after it has been freed/dropped would be undefined
    /// behavior, so this calls `zeroize()` explicitly on an owned, still-live
    /// value and inspects it before drop — the same operation `Zeroizing`'s
    /// own `Drop` impl performs internally.
    #[test]
    fn zeroize_actually_clears_key_bytes() {
        let mut derived = Zeroizing::new(vec![0xAAu8; KEY_SIZE]);
        assert!(derived.iter().all(|&b| b == 0xAA));
        derived.zeroize();
        assert!(
            derived.iter().all(|&b| b == 0),
            "zeroize() must clear every byte of derived key material"
        );
    }

    #[test]
    fn roundtrip() {
        let master = [42u8; 32];
        let crypto = SecretsCrypto::new(&master).unwrap();
        let plaintext = b"my-secret-api-key-value";
        let (packed, salt) = crypto.encrypt(plaintext).unwrap();
        let decrypted = crypto.decrypt(&packed, &salt).unwrap();
        assert_eq!(decrypted.as_slice(), plaintext);
    }

    #[test]
    fn different_salts_different_ciphertexts() {
        let master = [42u8; 32];
        let crypto = SecretsCrypto::new(&master).unwrap();
        let (a, sa) = crypto.encrypt(b"same").unwrap();
        let (b, sb) = crypto.encrypt(b"same").unwrap();
        assert_ne!(a, b); // different nonces/salts
        assert_ne!(sa, sb);
        assert_eq!(crypto.decrypt(&a, &sa).unwrap().as_slice(), b"same");
        assert_eq!(crypto.decrypt(&b, &sb).unwrap().as_slice(), b"same");
    }

    #[test]
    fn invalid_master_key() {
        assert!(SecretsCrypto::new(&[0u8; 16]).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let master = [42u8; 32];
        let crypto = SecretsCrypto::new(&master).unwrap();
        let (mut packed, salt) = crypto.encrypt(b"secret").unwrap();
        packed[15] ^= 0xFF; // flip a bit
        assert!(crypto.decrypt(&packed, &salt).is_err());
    }
}
