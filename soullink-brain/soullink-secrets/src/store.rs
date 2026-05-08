//! Persistent encrypted secret store backed by filesystem.
//! Stores each secret as a JSON file with encrypted value + salt.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::crypto::SecretsCrypto;
use crate::types::*;

/// File-based encrypted secret store.
pub struct SecretsStore {
    crypto: SecretsCrypto,
    dir: PathBuf,
    cache: HashMap<SecretId, (Vec<u8>, Vec<u8>)>, // (encrypted, salt)
}

impl SecretsStore {
    /// Create or open a secret store at `dir` with `master_key`.
    pub async fn open(dir: impl AsRef<Path>, master_key: &[u8]) -> Result<Self, SecretError> {
        let dir = dir.as_ref().to_path_buf();
        tokio::fs::create_dir_all(&dir).await?;

        let crypto = SecretsCrypto::new(master_key)?;
        let mut store = Self { crypto, dir, cache: HashMap::new() };

        // Load existing secrets
        let mut entries = tokio::fs::read_dir(&store.dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.path().extension().map(|e| e == "enc").unwrap_or(false) {
                if let Some(id) = entry.file_name().to_str().unwrap().strip_suffix(".enc") {
                    let data = tokio::fs::read(entry.path()).await?;
                    if let Ok((encrypted, salt)) = bincode::deserialize::<(Vec<u8>, Vec<u8>)>(&data) {
                        store.cache.insert(SecretId(id.to_string()), (encrypted, salt));
                    }
                }
            }
        }

        Ok(store)
    }

    /// Store a secret (encrypts and persists).
    pub async fn put(&mut self, id: &SecretId, value: &[u8]) -> Result<(), SecretError> {
        let (encrypted, salt) = self.crypto.encrypt(value)?;
        self.cache.insert(id.clone(), (encrypted.clone(), salt.clone()));

        let path = self.dir.join(format!("{}.enc", id.0));
        let data = bincode::serialize(&(encrypted, salt)).map_err(|e| SecretError::Serialization(e.to_string()))?;
        tokio::fs::write(path, data).await?;
        Ok(())
    }

    /// Retrieve and decrypt a secret.
    pub fn get(&self, id: &SecretId) -> Result<Vec<u8>, SecretError> {
        let (encrypted, salt) = self.cache.get(id)
            .ok_or_else(|| SecretError::NotFound(id.0.clone()))?;
        self.crypto.decrypt(encrypted, salt)
    }

    /// Delete a secret.
    pub async fn delete(&mut self, id: &SecretId) -> Result<(), SecretError> {
        self.cache.remove(id);
        let path = self.dir.join(format!("{}.enc", id.0));
        if path.exists() {
            tokio::fs::remove_file(path).await?;
        }
        Ok(())
    }

    /// List all secret IDs.
    pub fn list(&self) -> Vec<SecretId> {
        self.cache.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn store_roundtrip() {
        let dir = tempdir().unwrap();
        let master = [7u8; 32];
        let mut store = SecretsStore::open(dir.path(), &master).await.unwrap();
        let id = SecretId("api-key".into());
        store.put(&id, b"sk-12345").await.unwrap();
        assert_eq!(store.get(&id).unwrap(), b"sk-12345");
    }

    #[tokio::test]
    async fn persist_across_reopen() {
        let dir = tempdir().unwrap();
        let master = [7u8; 32];
        {
            let mut store = SecretsStore::open(dir.path(), &master).await.unwrap();
            store.put(&SecretId("token".into()), b"abc").await.unwrap();
        }
        let store = SecretsStore::open(dir.path(), &master).await.unwrap();
        assert_eq!(store.get(&SecretId("token".into())).unwrap(), b"abc");
    }

    #[tokio::test]
    async fn delete_secret() {
        let dir = tempdir().unwrap();
        let master = [7u8; 32];
        let mut store = SecretsStore::open(dir.path(), &master).await.unwrap();
        let id = SecretId("temp".into());
        store.put(&id, b"tmp").await.unwrap();
        store.delete(&id).await.unwrap();
        assert!(store.get(&id).is_err());
    }
}
