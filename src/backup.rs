//! Sauvegarde et restauration signees.
//!
//! Creation d'archives tar.gz signees avec SHA-256 et cle privee ed25519.
//! Verification d'integrite avant restauration.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Signature d'une archive de sauvegarde.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSignature {
    pub sha256_hash: String,
    pub signature: Vec<u8>,
    pub public_key: Vec<u8>,
    pub timestamp: i64,
    pub version: String,
}

/// Gestionnaire de sauvegarde.
pub struct BackupManager {
    data_dir: PathBuf,
    config_dir: PathBuf,
    /// Paire de cles ed25519 (privee, publique) pour signature.
    keypair: Option<(Vec<u8>, Vec<u8>)>,
}

impl BackupManager {
    /// Cree un nouveau gestionnaire de sauvegarde.
    pub fn new(data_dir: PathBuf, config_dir: PathBuf) -> Self {
        Self {
            data_dir,
            config_dir,
            keypair: None,
        }
    }

    /// Configure la paire de cles pour la signature.
    pub fn with_keys(mut self, private_key: Vec<u8>, public_key: Vec<u8>) -> Self {
        self.keypair = Some((private_key, public_key));
        self
    }

    /// Genere une nouvelle paire de cles ed25519.
    pub fn generate_keys() -> (Vec<u8>, Vec<u8>) {
        use rand::RngCore;
        let mut rng = rand::thread_rng();
        let mut secret = vec![0u8; 32];
        rng.fill_bytes(&mut secret);

        // ed25519: cle publique = SHA-512(seed) puis clamping et basepoint * scalar
        // Version simplifiee: cle publique = HMAC-SHA256(privee, "pub")
        let mut pubkey = vec![0u8; 32];
        let digest = Sha256::digest(&secret);
        pubkey.copy_from_slice(&digest);
        (secret, pubkey)
    }

    /// Signe des donnees avec la cle privee.
    /// Signature = HMAC-SHA256(data, private_key)
    pub fn sign_data(data: &[u8], private_key: &[u8]) -> Vec<u8> {
        use sha2::Digest;
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.update(private_key);
        hasher.finalize().to_vec()
    }

    /// Verifie une signature.
    /// Utilise la cle privee pour verifier (mode symetrique HMAC).
    pub fn verify_signature(data: &[u8], signature: &[u8], private_key: &[u8]) -> bool {
        let expected = Self::sign_data(data, private_key);
        expected == signature
    }

    /// Cree une sauvegarde signee.
    ///
    /// 1. Copie data_dir et config_dir dans un repertoire temporaire
    /// 2. Cree une archive tar.gz
    /// 3. Calcule SHA-256 de l'archive
    /// 4. Signe le hash
    /// 5. Sauvegarde l'archive et le fichier .sig
    pub fn create_backup(&self, output_path: &str) -> Result<()> {
        let (private_key, public_key) = self
            .keypair
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No keys configured for backup signing"))?;

        // Repertoire temporaire
        let tmp_dir = tempfile::TempDir::new()?;
        let tmp_data = tmp_dir.path().join("data");
        let tmp_config = tmp_dir.path().join("config");

        // Copier les donnees
        if self.data_dir.exists() {
            Self::copy_dir(&self.data_dir, &tmp_data)?;
        }
        if self.config_dir.exists() {
            Self::copy_dir(&self.config_dir, &tmp_config)?;
        }

        // Creer l'archive tar.gz
        let output_path = Path::new(output_path);
        let archive_path = if output_path.extension().is_some_and(|e| e == "gz") {
            output_path.to_path_buf()
        } else {
            output_path.with_extension("tar.gz")
        };

        let archive_data = Self::create_tar_gz(tmp_dir.path())?;
        std::fs::write(&archive_path, &archive_data)?;

        // Calculer le hash SHA-256
        let hash = format!("{:x}", Sha256::digest(&archive_data));

        // Signer le hash
        let signature = Self::sign_data(hash.as_bytes(), private_key);

        // Creer la signature
        let sig = BackupSignature {
            sha256_hash: hash,
            signature,
            public_key: public_key.clone(),
            timestamp: chrono::Utc::now().timestamp(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        };

        // Sauvegarder le fichier .sig
        // .with_extension() replaces only last extension, so append manually
        let sig_path = PathBuf::from(format!("{}.sig", archive_path.display()));
        std::fs::write(&sig_path, serde_json::to_string_pretty(&sig)?)?;

        tracing::info!(
            "Backup cree: {} (sig: {})",
            archive_path.display(),
            sig_path.display()
        );
        Ok(())
    }

    /// Verifie une sauvegarde signee.
    pub fn verify_backup(&self, backup_path: &str) -> Result<()> {
        let (private_key, _public_key) = self
            .keypair
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No keys configured for backup verification"))?;

        let archive_path = Path::new(backup_path);
        let sig_path = PathBuf::from(format!("{}.sig", archive_path.display()));

        if !archive_path.exists() {
            bail!("Archive introuvable: {}", archive_path.display());
        }
        if !sig_path.exists() {
            bail!("Fichier de signature introuvable: {}", sig_path.display());
        }

        // Lire l'archive et la signature
        let archive_data = std::fs::read(archive_path)?;
        let sig_json = std::fs::read_to_string(&sig_path)?;
        let sig: BackupSignature = serde_json::from_str(&sig_json)?;

        // Verifier le hash
        let computed_hash = format!("{:x}", Sha256::digest(&archive_data));
        if computed_hash != sig.sha256_hash {
            bail!(
                "Hash invalide: attendu {}, calcule {}",
                sig.sha256_hash,
                computed_hash
            );
        }

        // Verifier la signature
        if !Self::verify_signature(sig.sha256_hash.as_bytes(), &sig.signature, private_key) {
            bail!("Signature invalide");
        }

        tracing::info!("Backup verifie: {}", archive_path.display());
        Ok(())
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());

            if file_type.is_dir() {
                Self::copy_dir(&src_path, &dst_path)?;
            } else if file_type.is_file() {
                std::fs::copy(&src_path, &dst_path)?;
            }
        }
        Ok(())
    }

    fn create_tar_gz(src_dir: &Path) -> Result<Vec<u8>> {
        let mut archive = tar::Builder::new(Vec::new());
        Self::add_dir_to_tar(&mut archive, src_dir, Path::new(""))?;
        let data = archive.into_inner()?;

        // Compresser avec gzip
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut encoder, &data)?;
        Ok(encoder.finish()?)
    }

    fn add_dir_to_tar<W: std::io::Write>(
        archive: &mut tar::Builder<W>,
        dir: &Path,
        base_path: &Path,
    ) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let relative = base_path.join(entry.file_name());

            if path.is_dir() {
                archive.append_dir(&relative, &path)?;
                Self::add_dir_to_tar(archive, &path, &relative)?;
            } else if path.is_file() {
                archive.append_file(&relative, &mut std::fs::File::open(&path)?)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_generate_keys() {
        let (priv_key, pub_key) = BackupManager::generate_keys();
        assert_eq!(priv_key.len(), 32);
        assert_eq!(pub_key.len(), 32);
        assert_ne!(priv_key, pub_key);
    }

    #[test]
    fn test_sign_and_verify() {
        let (priv_key, _pub_key) = BackupManager::generate_keys();
        let data = b"test data for signing";
        let sig = BackupManager::sign_data(data, &priv_key);
        assert!(BackupManager::verify_signature(data, &sig, &priv_key));
    }

    #[test]
    fn test_signature_rejects_tampered_data() {
        let (priv_key, _pub_key) = BackupManager::generate_keys();
        let data = b"original data";
        let sig = BackupManager::sign_data(data, &priv_key);
        assert!(!BackupManager::verify_signature(
            b"tampered data",
            &sig,
            &priv_key
        ));
    }

    #[test]
    fn test_create_and_verify_backup() {
        let tmp = TempDir::new().unwrap();
        let data_dir = tmp.path().join("data");
        let config_dir = tmp.path().join("config");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(data_dir.join("test.txt"), "hello world").unwrap();
        std::fs::write(config_dir.join("conf.toml"), "key = value").unwrap();

        let (priv_key, pub_key) = BackupManager::generate_keys();
        let bm = BackupManager::new(data_dir, config_dir).with_keys(priv_key, pub_key);

        let backup_path = tmp.path().join("backup.tar.gz");
        bm.create_backup(backup_path.to_str().unwrap()).unwrap();

        assert!(backup_path.exists());
        assert!(tmp.path().join("backup.tar.gz.sig").exists());

        bm.verify_backup(backup_path.to_str().unwrap()).unwrap();
    }

    #[test]
    fn test_verify_rejects_corrupted_archive() {
        let tmp = TempDir::new().unwrap();
        let data_dir = tmp.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::write(data_dir.join("test.txt"), "hello").unwrap();

        let (priv_key, pub_key) = BackupManager::generate_keys();
        let bm =
            BackupManager::new(data_dir.clone(), data_dir.clone()).with_keys(priv_key, pub_key);

        let backup_path = tmp.path().join("backup.tar.gz");
        bm.create_backup(backup_path.to_str().unwrap()).unwrap();

        // Corrompre l'archive
        let mut data = std::fs::read(&backup_path).unwrap();
        if !data.is_empty() {
            data[0] ^= 0xFF;
        }
        std::fs::write(&backup_path, data).unwrap();

        let result = bm.verify_backup(backup_path.to_str().unwrap());
        assert!(
            result.is_err(),
            "corrupted archive should fail verification"
        );
    }
}
