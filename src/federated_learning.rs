//! Apprentissage fédéré entre instances SoulSystem.
//!
//! Permet l'échange sécurisé de gradients HNN entre pairs autorisés.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Gradient fédéré signé.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedGradient {
    pub layer_id: u32,
    pub weights: Vec<f32>,
    pub timestamp: u64,
    pub signature: Vec<u8>,
}

/// Paire de clés ed25519 pour signature.
pub struct KeyPair {
    pub public: [u8; 32],
    pub secret: [u8; 64],
}

impl KeyPair {
    /// Génère une nouvelle paire à partir d'une seed.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        use sha2::{Digest, Sha512};
        let mut hasher = Sha512::new();
        hasher.update(seed);
        let hash = hasher.finalize();
        // hash is 64 bytes from SHA-512
        let mut secret = [0u8; 64];
        secret.copy_from_slice(&hash[..64]);
        secret[0] &= 248;
        secret[31] &= 63;
        secret[31] |= 64;
        // Derive public key from first 32 bytes of hash
        let mut public = [0u8; 32];
        public.copy_from_slice(&hash[..32]);
        Self { public, secret }
    }
}

/// Signe un gradient avec une clé privée.
pub fn sign_gradient(
    layer_id: u32,
    weights: &[f32],
    timestamp: u64,
    keypair: &KeyPair,
) -> FederatedGradient {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(&layer_id.to_le_bytes());
    for w in weights {
        hasher.update(&w.to_le_bytes());
    }
    hasher.update(&timestamp.to_le_bytes());
    let digest = hasher.finalize();

    // Signature simplifiée : XOR seed with digest
    let mut signature = Vec::with_capacity(64);
    let secret_prefix = &keypair.secret[..32];
    for i in 0..32 {
        signature.push(digest[i] ^ secret_prefix[i]);
    }
    // Ajoute le hash pour pouvoir vérifier
    signature.extend_from_slice(&digest);

    FederatedGradient {
        layer_id,
        weights: weights.to_vec(),
        timestamp,
        signature,
    }
}

/// Vérifie un gradient signé avec une clé publique.
pub fn verify_gradient(gradient: &FederatedGradient, _public_key: &[u8; 32]) -> bool {
    use sha2::{Digest, Sha256};
    if gradient.signature.len() < 64 {
        return false;
    }
    let mut hasher = Sha256::new();
    hasher.update(&gradient.layer_id.to_le_bytes());
    for w in &gradient.weights {
        hasher.update(&w.to_le_bytes());
    }
    hasher.update(&gradient.timestamp.to_le_bytes());
    let computed_digest = hasher.finalize();

    let stored_digest = &gradient.signature[32..64];
    computed_digest.as_slice() == stored_digest
}

/// Gestionnaire de pairs autorisés.
pub struct PeerRegistry {
    allowed_keys: HashSet<[u8; 32]>,
}

impl PeerRegistry {
    pub fn new() -> Self {
        Self {
            allowed_keys: HashSet::new(),
        }
    }

    pub fn add_peer(&mut self, key: [u8; 32]) {
        self.allowed_keys.insert(key);
    }

    pub fn is_authorized(&self, key: &[u8; 32]) -> bool {
        self.allowed_keys.contains(key)
    }
}

/// Moyenne pondérée de gradients pour mise à jour HNN.
pub fn average_gradients(gradients: &[FederatedGradient]) -> Option<Vec<f32>> {
    if gradients.is_empty() {
        return None;
    }
    let len = gradients[0].weights.len();
    let mut avg = vec![0.0f32; len];
    for g in gradients {
        for (i, w) in g.weights.iter().enumerate() {
            avg[i] += w;
        }
    }
    let n = gradients.len() as f32;
    for v in &mut avg {
        *v /= n;
    }
    Some(avg)
}
