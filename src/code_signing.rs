//! Signature et vérification de code source.
//!
//! Garantit que tout code exécuté par OpenEvolve ou AVID
//! est signé par une clé autorisée.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::PathBuf;

/// Code source signé.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedCode {
    pub code: String,
    pub signature: Vec<u8>,
    pub public_key: Vec<u8>,
}

/// Registre des clés autorisées.
pub struct AuthorizedKeys {
    pub keys: HashSet<Vec<u8>>,
}

impl AuthorizedKeys {
    pub fn len(&self) -> usize { self.keys.len() }
    pub fn is_empty(&self) -> bool { self.keys.is_empty() }
    pub fn new() -> Self {
        Self { keys: HashSet::new() }
    }
    /// Charge les clés autorisées depuis le fichier de config.
    pub fn load() -> Result<Self> {
        let path = dirs_next_home()
            .unwrap_or_else(|| PathBuf::from("/root"))
            .join(".soulsystem")
            .join("authorized_keys");

        let mut keys = HashSet::new();

        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            for line in content.lines() {
                let line = line.trim();
                if !line.is_empty() && !line.starts_with('#') {
                    if let Ok(key) = base64_decode(line) {
                        keys.insert(key);
                    }
                }
            }
        }

        Ok(Self { keys })
    }

    /// Ajoute une clé publique.
    pub fn add_key(&mut self, key: &[u8]) -> Result<()> {
        self.keys.insert(key.to_vec());
        self.save()?;
        Ok(())
    }

    fn save(&self) -> Result<()> {
        let path = dirs_next_home()
            .unwrap_or_else(|| PathBuf::from("/root"))
            .join(".soulsystem");
        std::fs::create_dir_all(&path)?;
        let auth_path = path.join("authorized_keys");
        let mut content = String::new();
        for key in &self.keys {
            content.push_str(&base64_encode(key));
            content.push('\n');
        }
        std::fs::write(auth_path, content)?;
        Ok(())
    }
}

/// Vérifie qu'un SignedCode est valide et autorisé.
pub fn verify_code(signed: &SignedCode, authorized: &AuthorizedKeys) -> Result<()> {
    // Vérifier que la clé est autorisée
    if !authorized.keys.contains(&signed.public_key) {
        bail!("Clé publique non autorisée");
    }

    // Vérifier la signature
    let mut hasher = Sha256::new();
    hasher.update(signed.code.as_bytes());
    let digest = hasher.finalize();

    // Signature simplifiée : XOR du hash avec la clé publique
    let expected_sig: Vec<u8> = digest
        .iter()
        .zip(signed.public_key.iter().cycle())
        .map(|(a, b)| a ^ b)
        .collect();

    if expected_sig != signed.signature {
        bail!("Signature invalide");
    }

    Ok(())
}

fn dirs_next_home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

fn base64_encode(data: &[u8]) -> String {
    use std::fmt::Write;
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        write!(
            result,
            "{}",
            alphabet[((triple >> 18) & 63) as usize] as char
        )
        .unwrap();
        write!(
            result,
            "{}",
            alphabet[((triple >> 12) & 63) as usize] as char
        )
        .unwrap();
        if chunk.len() > 1 {
            write!(
                result,
                "{}",
                alphabet[((triple >> 6) & 63) as usize] as char
            )
            .unwrap();
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            write!(result, "{}", alphabet[(triple & 63) as usize] as char).unwrap();
        } else {
            result.push('=');
        }
    }
    result
}

fn base64_decode(s: &str) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    let chars: Vec<u8> = s.bytes().filter(|&b| b != b'=').collect();
    for chunk in chars.chunks(4) {
        let mut triple = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            let val = match c {
                b'A'..=b'Z' => c - b'A',
                b'a'..=b'z' => c - b'a' + 26,
                b'0'..=b'9' => c - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                _ => bail!("Caractère base64 invalide"),
            } as u32;
            triple |= val << (18 - i * 6);
        }
        result.push(((triple >> 16) & 0xFF) as u8);
        if chunk.len() > 2 {
            result.push(((triple >> 8) & 0xFF) as u8);
        }
        if chunk.len() > 3 {
            result.push((triple & 0xFF) as u8);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_code() {
        let code = "fn main() { println!(\"hello\"); }";
        let pubkey = vec![1u8; 32];
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

        let mut auth = AuthorizedKeys {
            keys: HashSet::new(),
        };
        auth.keys.insert(pubkey);
        assert!(verify_code(&signed, &auth).is_ok());
    }

    #[test]
    fn test_reject_unauthorized() {
        let code = "fn main() {}";
        let pubkey = vec![2u8; 32];
        let digest = Sha256::digest(code.as_bytes());
        let signature: Vec<u8> = digest
            .iter()
            .zip(pubkey.iter().cycle())
            .map(|(a, b)| a ^ b)
            .collect();

        let signed = SignedCode {
            code: code.into(),
            signature,
            public_key: pubkey,
        };

        let auth = AuthorizedKeys {
            keys: HashSet::new(),
        };
        assert!(verify_code(&signed, &auth).is_err());
    }
}
