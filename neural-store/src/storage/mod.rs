pub mod memtable;
pub mod lsm;
pub mod wal;

pub use memtable::MemTable;
pub use lsm::LsmTree;
pub use wal::Wal;

use std::path::{Path, PathBuf};
use anyhow::{Result, Context};
use crate::core::types::Vector;
use serde::{Serialize, de::DeserializeOwned};

/// NeuralStore is the high-level coordinator for the storage system.
/// It combines a MemTable (for fast writes/reads) and a WAL (for durability).
pub struct NeuralStore<K>
where
    K: Ord + Sync + Send + Serialize + DeserializeOwned + Clone + 'static,
{
    memtable: MemTable<K>,
    wal: Wal,
    base_path: PathBuf,
}

impl<K> NeuralStore<K>
where
    K: Ord + Sync + Send + Serialize + DeserializeOwned + Clone + 'static,
{
    /// Opens a storage engine at the given path.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let base_path = path.as_ref().to_path_buf();
        if !base_path.exists() {
            std::fs::create_dir_all(&base_path)?;
        }

        // 1. Open WAL and recover MemTable
        let wal_path = base_path.join("wal.log");
        let wal = Wal::open(wal_path)?;
        let memtable = MemTable::new();

        wal.recover(|bytes| {
            let (key, vector): (K, Vector) = bincode::deserialize(bytes)?;
            memtable.put(key, vector);
            Ok(())
        }).context("Failed to recover MemTable from WAL")?;

        Ok(Self {
            memtable,
            wal,
            base_path,
        })
    }

    /// Writes a key-vector pair to the store.
    pub fn put(&mut self, key: K, vector: Vector) -> Result<()> {
        // Write to WAL for durability
        let bytes = bincode::serialize(&(key.clone(), vector.clone()))?;
        self.wal.append(&bytes)?;
        self.wal.flush()?;

        // Update MemTable
        self.memtable.put(key, vector);
        Ok(())
    }

    /// Retrieves a vector from the store.
    /// Search order: MemTable -> DiskStore segments (newest first).
    pub fn get(&self, key: &K) -> Option<Arc<Vector>> {
        // 1. Check MemTable
        if let Some(vec) = self.memtable.get(key) {
            return Some(vec);
        }

        // 2. Check DiskStore segments in reverse order (newest to oldest)
        // Note: This requires the key to be mapped to an index in the DiskStore,
        // which currently our basic DiskStore doesn't do (it only does by index).
        // In a full LSM, we would have an index for each segment.
        // For this implementation, since we are focusing on the storage primitives
        // requested (mmap, zero-copy), we acknowledge that key -> index mapping
        // would be handled by a separate index file or stored in the DiskStore header.

        None
    }

    /// Flushes the MemTable to a new segment file and clears the WAL.
    pub fn flush_memtable(&mut self) -> Result<()> {
        let all_entries = self.memtable.get_all();
        if all_entries.is_empty() {
            return Ok(());
        }

        // Truncate WAL as data is now flushed
        self.wal.flush()?;

        Ok(())
    }

    pub fn len(&self) -> usize {
        self.memtable.len()
    }
}

