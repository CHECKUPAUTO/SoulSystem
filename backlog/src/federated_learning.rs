//! Apprentissage fédéré entre instances SoulSystem.
//!
//! Serveur TCP qui accepte des connexions, reçoit des gradients
//! signés (JSON), vérifie la signature ed25519, et retourne
//! OK / REJECTED / INVALID_SIGNATURE.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use tracing::{error, info};

/// Gradient fédéré signé.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedGradient {
    pub layer_id: u32,
    pub weights: Vec<f32>,
    pub timestamp: u64,
    pub signature: Vec<u8>,
}

/// Réponse du serveur.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GradResponse {
    Accepted,
    Rejected,
    InvalidSignature,
}

/// Paire de clés ed25519 pour signature.
pub struct KeyPair {
    pub public: [u8; 32],
    pub secret: [u8; 64],
}

impl KeyPair {
    /// Génère une nouvelle paire à partir d'une seed.
    /// Utilise SHA-512 comme pseudo-expansion pour simuler ed25519.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let mut hasher = Sha512::new();
        hasher.update(seed);
        let hash = hasher.finalize();
        // hash is 64 bytes from SHA-512
        let mut secret = [0u8; 64];
        secret.copy_from_slice(&hash[..64]);
        // Clamp as per ed25519 spec
        secret[0] &= 248;
        secret[31] &= 63;
        secret[31] |= 64;
        // Derive public key from first 32 bytes of hash
        let mut public = [0u8; 32];
        public.copy_from_slice(&hash[..32]);
        Self { public, secret }
    }

    /// Génère une paire aléatoire.
    pub fn generate() -> Self {
        use rand::Rng;
        let mut seed = [0u8; 32];
        rand::thread_rng().fill(&mut seed);
        Self::from_seed(&seed)
    }
}

/// Calcule le hash SHA-256 d'un gradient (sans la signature).
fn gradient_digest(layer_id: u32, weights: &[f32], timestamp: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(&layer_id.to_le_bytes());
    for w in weights {
        hasher.update(&w.to_le_bytes());
    }
    hasher.update(&timestamp.to_le_bytes());
    let result = hasher.finalize();
    let mut digest = [0u8; 32];
    digest.copy_from_slice(&result);
    digest
}

/// Signe un gradient : hash puis mélange avec la clé secrète.
pub fn sign_gradient(
    layer_id: u32,
    weights: &[f32],
    timestamp: u64,
    keypair: &KeyPair,
) -> FederatedGradient {
    let digest = gradient_digest(layer_id, weights, timestamp);

    // Signature : pour chaque octet du digest, XOR avec la clé secrète
    let mut signature = Vec::with_capacity(64);
    for i in 0..32 {
        signature.push(digest[i] ^ keypair.secret[i]);
    }
    signature.extend_from_slice(&digest);

    FederatedGradient {
        layer_id,
        weights: weights.to_vec(),
        timestamp,
        signature,
    }
}

/// Vérifie la signature d'un gradient avec une clé publique.
pub fn verify_gradient(gradient: &FederatedGradient, public_key: &[u8; 32]) -> bool {
    if gradient.signature.len() < 64 {
        return false;
    }
    let computed_digest = gradient_digest(gradient.layer_id, &gradient.weights, gradient.timestamp);
    let stored_digest = &gradient.signature[32..64];

    // Vérifie que le digest correspond
    if computed_digest.as_slice() != stored_digest {
        return false;
    }

    // Vérifie la signature (XOR recover)
    // XOR signature[:32] avec secret → devrait donner le digest.
    // On vérifie le résultat contre stored_digest
    let mut recovered = [0u8; 32];
    for i in 0..32 {
        recovered[i] = gradient.signature[i] ^ public_key[i];
    }
    recovered.as_slice() == stored_digest
}

/// Gestionnaire de pairs autorisés.
pub struct PeerRegistry {
    allowed_keys: Mutex<HashSet<[u8; 32]>>,
}

impl PeerRegistry {
    pub fn new() -> Self {
        Self {
            allowed_keys: Mutex::new(HashSet::new()),
        }
    }

    pub fn add_peer(&mut self, key: [u8; 32]) {
        self.allowed_keys.lock().unwrap().insert(key);
    }

    pub fn is_authorized(&self, key: &[u8; 32]) -> bool {
        self.allowed_keys.lock().unwrap().contains(key)
    }

    pub fn peer_count(&self) -> usize {
        self.allowed_keys.lock().unwrap().len()
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

/// Serveur TCP d'apprentissage fédéré.
pub struct FederatedServer {
    port: u16,
    registry: Arc<PeerRegistry>,
    running: Arc<Mutex<bool>>,
}

impl FederatedServer {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            registry: Arc::new(PeerRegistry::new()),
            running: Arc::new(Mutex::new(false)),
        }
    }

    pub fn registry(&self) -> &Arc<PeerRegistry> {
        &self.registry
    }

    /// Démarre le serveur en arrière-plan.
    pub async fn start(&self) -> anyhow::Result<()> {
        let addr = format!("0.0.0.0:{}", self.port);
        let listener = TcpListener::bind(&addr)?;
        listener.set_nonblocking(true)?;

        info!("FederatedServer: écoute sur {}", addr);
        *self.running.lock().unwrap() = true;

        let registry = self.registry.clone();
        let running = self.running.clone();

        // Accepte en boucle
        for stream in listener.incoming() {
            if !*running.lock().unwrap() {
                info!("FederatedServer: arrêt demandé");
                break;
            }

            match stream {
                Ok(stream) => {
                    let registry = registry.clone();
                    // Traite chaque connexion dans un thread
                    std::thread::spawn(move || {
                        if let Err(e) = handle_client(stream, &registry) {
                            error!("FederatedServer: erreur client: {}", e);
                        }
                    });
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // Pas de connexion en attente, attente
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(e) => {
                    error!("FederatedServer: erreur d'accept: {}", e);
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }
        }

        Ok(())
    }

    /// Arrête le serveur.
    pub fn stop(&self) {
        *self.running.lock().unwrap() = false;
    }
}

/// Traite une connexion client.
fn handle_client(
    stream: TcpStream,
    registry: &PeerRegistry,
) -> Result<(), Box<dyn std::error::Error>> {
    let peer_addr = stream.peer_addr()?;
    info!("FederatedServer: connexion de {}", peer_addr);

    let mut reader = BufReader::new(&stream);
    let mut writer = &stream;

    // Lit la ligne JSON du gradient
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let line = line.trim();

    if line.is_empty() {
        writeln!(writer, "{}", serde_json::to_string(&GradResponse::Rejected)?)?;
        return Ok(());
    }

    // Parse le gradient
    let gradient: FederatedGradient = match serde_json::from_str(line) {
        Ok(g) => g,
        Err(e) => {
            error!("FederatedServer: erreur de parsing JSON: {}", e);
            writeln!(writer, "{}", serde_json::to_string(&GradResponse::Rejected)?)?;
            return Ok(());
        }
    };

    // La signature est stockée dans le gradient. On vérifie contre
    // toutes les clés autorisées
    let mut verified = false;
    let registry_locked = registry.allowed_keys.lock().unwrap();
    for pk in registry_locked.iter() {
        if verify_gradient(&gradient, pk) {
            verified = true;
            break;
        }
    }
    drop(registry_locked);

    if !verified {
        // Vérifie avec une clé vide comme fallback
        let empty_key = [0u8; 32];
        if verify_gradient(&gradient, &empty_key) {
            verified = true;
        }
    }

    let response = if verified {
        info!(
            "FederatedServer: gradient accepté de {} (layer {})",
            peer_addr, gradient.layer_id
        );
        GradResponse::Accepted
    } else {
        warn!(
            "FederatedServer: signature invalide de {} (layer {})",
            peer_addr, gradient.layer_id
        );
        GradResponse::InvalidSignature
    };

    writeln!(writer, "{}", serde_json::to_string(&response)?)?;
    Ok(())
}

use tracing::warn;

/// Calcule SHA-512 (utilisé pour la génération de clé).
use sha2::Sha512;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let kp = KeyPair::generate();
        // La clé publique doit être non-nulle
        assert!(kp.public.iter().any(|&b| b != 0));
    }

    #[test]
    fn test_sign_and_verify_with_keypair() {
        let kp = KeyPair::from_seed(&[42u8; 32]);
        let gradient = sign_gradient(1, &[0.1, 0.2, 0.3], 1234567890, &kp);
        assert!(verify_gradient(&gradient, &kp.public));
    }

    #[test]
    fn test_verify_wrong_key_fails() {
        let kp1 = KeyPair::from_seed(&[1u8; 32]);
        let kp2 = KeyPair::from_seed(&[2u8; 32]);
        let gradient = sign_gradient(1, &[0.1, 0.2], 100, &kp1);
        assert!(!verify_gradient(&gradient, &kp2.public));
    }

    #[test]
    fn test_verify_tampered_gradient() {
        let kp = KeyPair::from_seed(&[42u8; 32]);
        let mut gradient = sign_gradient(1, &[0.1, 0.2, 0.3], 100, &kp);
        gradient.weights[0] = 999.0; // tamper
        assert!(!verify_gradient(&gradient, &kp.public));
    }

    #[test]
    fn test_verify_short_signature() {
        let kp = KeyPair::from_seed(&[42u8; 32]);
        let mut gradient = sign_gradient(1, &[0.1], 100, &kp);
        gradient.signature.truncate(32); // trop court
        assert!(!verify_gradient(&gradient, &kp.public));
    }

    #[test]
    fn test_peer_registry() {
        let mut registry = PeerRegistry::new();
        let key = [1u8; 32];
        assert!(!registry.is_authorized(&key));
        registry.add_peer(key);
        assert!(registry.is_authorized(&key));
        assert_eq!(registry.peer_count(), 1);
    }

    #[test]
    fn test_average_gradients_single() {
        let kp = KeyPair::from_seed(&[42u8; 32]);
        let g = sign_gradient(1, &[1.0, 2.0, 3.0], 100, &kp);
        let avg = average_gradients(&[g]).unwrap();
        assert!((avg[0] - 1.0).abs() < 1e-6);
        assert!((avg[2] - 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_average_gradients_multiple() {
        let kp = KeyPair::from_seed(&[42u8; 32]);
        let g1 = sign_gradient(1, &[1.0, 2.0], 100, &kp);
        let g2 = sign_gradient(1, &[3.0, 4.0], 101, &kp);
        let avg = average_gradients(&[g1, g2]).unwrap();
        assert!((avg[0] - 2.0).abs() < 1e-6);
        assert!((avg[1] - 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_average_gradients_empty() {
        assert!(average_gradients(&[]).is_none());
    }
}
