//! AES-256-GCM encryption with per-secret HKDF-SHA256 key derivation.
//! Ported from IronClaw secrets/crypto.rs.

use aes_gcm::{
    aead::{Aead, AeadCore, OsRng},
    Aes256Gcm, KeyInit, Nonce,
};
use hkdf::Hkdf;
use sha2::Sha256;

use crate::types::SecretError;

const KEY_SIZE: usize = 32;
const NONCE_SIZE: usize = 12;
const SALT_SIZE: usize = 32;
const INFO: &[u8] = b"soullink-secret-v1";

/// Cryptographic engine for secret encryption.
pub struct SecretsCrypto {
    master_key: Vec<u8>,
}

impl SecretsCrypto {
    pub fn new(master_key: &[u8]) -> Result<Self, SecretError> {
        if master_key.len() < KEY_SIZE {
            return Err(SecretError::InvalidMasterKey);
        }
        Ok(Self { master_key: master_key.to_vec() })
    }

    /// Generate a random salt.
    pub fn generate_salt() -> Vec<u8> {
        let mut salt = vec![0u8; SALT_SIZE];
        rand::RngCore::fill_bytes(&mut OsRng, &mut salt);
        salt
    }

    /// Derive a per-secret key using HKDF-SHA256.
    fn derive_key(&self, salt: &[u8]) -> Result<Vec<u8>, SecretError> {
        let hkdf = Hkdf::<Sha256>::new(Some(salt), &self.master_key);
        let mut derived = vec![0u8; KEY_SIZE];
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

    /// Decrypt packed ciphertext with salt. Returns plaintext.
    pub fn decrypt(&self, packed: &[u8], salt: &[u8]) -> Result<Vec<u8>, SecretError> {
        let derived_key = self.derive_key(salt)?;

        let cipher = Aes256Gcm::new_from_slice(&derived_key)
            .map_err(|e| SecretError::DecryptionFailed(format!("Cipher init failed: {}", e)))?;

        if packed.len() < NONCE_SIZE {
            return Err(SecretError::DecryptionFailed("Packed data too short".into()));
        }

        let nonce = Nonce::from_slice(&packed[..NONCE_SIZE]);
        let ciphertext = &packed[NONCE_SIZE..];

        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| SecretError::DecryptionFailed(format!("Decrypt failed: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let master = [42u8; 32];
        let crypto = SecretsCrypto::new(&master).unwrap();
        let plaintext = b"my-secret-api-key-value";
        let (packed, salt) = crypto.encrypt(plaintext).unwrap();
        let decrypted = crypto.decrypt(&packed, &salt).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn different_salts_different_ciphertexts() {
        let master = [42u8; 32];
        let crypto = SecretsCrypto::new(&master).unwrap();
        let (a, sa) = crypto.encrypt(b"same").unwrap();
        let (b, sb) = crypto.encrypt(b"same").unwrap();
        assert_ne!(a, b); // different nonces/salts
        assert_ne!(sa, sb);
        assert_eq!(crypto.decrypt(&a, &sa).unwrap(), b"same");
        assert_eq!(crypto.decrypt(&b, &sb).unwrap(), b"same");
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
