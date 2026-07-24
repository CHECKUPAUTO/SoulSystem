use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::instrument;

use crate::config::MemoryConfig;
use crate::Data;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: u64,
    #[serde(with = "data_serde")]
    pub data: Data,
    pub strength: f64,
    pub access_count: u64,
    #[serde(skip, default = "Instant::now")]
    pub last_access: Instant,
}

impl MemoryEntry {
    fn new(id: u64, data: Data, initial_strength: f64) -> Self {
        MemoryEntry {
            id,
            data,
            strength: initial_strength,
            access_count: 0,
            last_access: Instant::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    #[serde(with = "entries_serde")]
    active: Vec<MemoryEntry>,
    #[serde(with = "entries_serde")]
    long_term: Vec<MemoryEntry>,
    next_id: u64,
    active_capacity: usize,
    consolidation_threshold: f64,
    decay_rate: f64,
    initial_strength: f64,
}

impl Memory {
    pub fn new(config: &MemoryConfig) -> Self {
        Memory {
            active: Vec::with_capacity(config.active_capacity),
            long_term: Vec::new(),
            next_id: 0,
            active_capacity: config.active_capacity,
            consolidation_threshold: config.consolidation_threshold,
            decay_rate: config.decay_rate,
            initial_strength: config.initial_strength,
        }
    }

    #[instrument(skip(self))]
    pub fn store(&mut self, data: Data) {
        let entry = MemoryEntry::new(self.next_id, data, self.initial_strength);
        self.next_id += 1;

        if self.active.len() < self.active_capacity {
            self.active.push(entry);
        } else {
            let weakest_idx = self
                .active
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.strength.partial_cmp(&b.strength).unwrap())
                .map(|(i, _)| i)
                .unwrap_or(0);

            let weakest = self.active.swap_remove(weakest_idx);
            if weakest.strength >= self.consolidation_threshold {
                self.long_term.push(weakest);
            }
            self.active.push(entry);
        }
    }

    #[instrument(skip(self))]
    pub fn store_with_priority(&mut self, data: Data, strength: f64) {
        let mut entry = MemoryEntry::new(self.next_id, data, strength);
        entry.strength = strength;
        self.next_id += 1;
        self.active.push(entry);
        if self.active.len() > self.active_capacity * 2 {
            self.trim_active();
        }
    }

    #[instrument(skip(self))]
    pub fn consolidate(&mut self) {
        let mut i = 0;
        while i < self.active.len() {
            if self.active[i].strength >= self.consolidation_threshold {
                let entry = self.active.remove(i);
                self.long_term.push(entry);
            } else {
                i += 1;
            }
        }
        self.long_term.sort_by(|a, b| {
            b.strength
                .partial_cmp(&a.strength)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.trim_active();
    }

    #[instrument(skip(self))]
    pub fn decay(&mut self) {
        self.active.iter_mut().for_each(|e| {
            e.strength = (e.strength - self.decay_rate).max(0.0);
        });
        self.long_term.iter_mut().for_each(|e| {
            e.strength = (e.strength - self.decay_rate * 0.5).max(0.0);
        });
        self.active
            .retain(|e| e.strength > 0.0 || e.access_count > 0);
        self.long_term.retain(|e| e.strength > 0.05);
    }

    pub fn recall(&mut self, key: &[u8]) -> Option<Data> {
        if let Some(idx) = self.active.iter().position(|e| e.data.payload == key) {
            let entry = &mut self.active[idx];
            entry.access_count += 1;
            entry.last_access = Instant::now();
            entry.strength = (entry.strength + 0.5).min(10.0);
            return Some(entry.data.clone());
        }
        if let Some(idx) = self.long_term.iter().position(|e| e.data.payload == key) {
            let mut entry = self.long_term.remove(idx);
            entry.access_count += 1;
            entry.last_access = Instant::now();
            entry.strength = (entry.strength + 0.5).min(10.0);
            if self.active.len() >= self.active_capacity {
                let weakest_idx = self
                    .active
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| a.strength.partial_cmp(&b.strength).unwrap())
                    .map(|(i, _)| i)
                    .unwrap();
                let weakest = self.active.swap_remove(weakest_idx);
                if weakest.strength >= self.consolidation_threshold {
                    self.long_term.push(weakest);
                }
            }
            self.active.push(entry);
            return Some(self.active.last().unwrap().data.clone());
        }
        None
    }

    pub fn recall_by_tag(&mut self, tag: &str) -> Vec<Data> {
        let mut results = Vec::new();
        for entry in &mut self.active {
            if entry.data.metadata.tags.iter().any(|t| t == tag) {
                entry.access_count += 1;
                entry.last_access = Instant::now();
                entry.strength = (entry.strength + 0.3).min(10.0);
                results.push(entry.data.clone());
            }
        }
        for entry in &mut self.long_term {
            if entry.data.metadata.tags.iter().any(|t| t == tag) {
                entry.access_count += 1;
                entry.last_access = Instant::now();
                entry.strength = (entry.strength + 0.3).min(10.0);
                results.push(entry.data.clone());
            }
        }
        results
    }

    pub fn snapshot(&self) -> (&[MemoryEntry], &[MemoryEntry]) {
        (&self.active, &self.long_term)
    }

    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    pub fn long_term_count(&self) -> usize {
        self.long_term.len()
    }

    pub fn total_size(&self) -> usize {
        self.active.len() + self.long_term.len()
    }

    fn trim_active(&mut self) {
        while self.active.len() > self.active_capacity {
            let weakest_idx = self
                .active
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.strength.partial_cmp(&b.strength).unwrap())
                .map(|(i, _)| i)
                .unwrap_or(0);
            let weakest = self.active.swap_remove(weakest_idx);
            if weakest.strength >= self.consolidation_threshold {
                self.long_term.push(weakest);
            }
        }
    }
}

mod data_serde {
    use crate::Data;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(data: &Data, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(&data.payload)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Data, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        Ok(Data::new(bytes))
    }
}

mod entries_serde {
    use crate::memory::MemoryEntry;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(entries: &[MemoryEntry], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let data: Vec<_> = entries
            .iter()
            .map(|e| MemoryEntrySerializable {
                id: e.id,
                payload: e.data.payload.clone(),
                metadata: e.data.metadata.clone(),
                strength: e.strength,
                access_count: e.access_count,
            })
            .collect();
        data.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<MemoryEntry>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entries: Vec<MemoryEntrySerializable> = Vec::deserialize(deserializer)?;
        Ok(entries
            .into_iter()
            .map(|e| MemoryEntry {
                id: e.id,
                data: crate::Data::new(e.payload).with_metadata(e.metadata),
                strength: e.strength,
                access_count: e.access_count,
                last_access: std::time::Instant::now(),
            })
            .collect())
    }

    #[derive(Serialize, Deserialize)]
    struct MemoryEntrySerializable {
        id: u64,
        payload: Vec<u8>,
        metadata: crate::DataMetadata,
        strength: f64,
        access_count: u64,
    }
}

impl Memory {
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)
    }

    pub fn load(path: &std::path::Path, config: &MemoryConfig) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        let mut memory: Self = serde_json::from_str(&json)?;
        memory.active_capacity = config.active_capacity;
        memory.consolidation_threshold = config.consolidation_threshold;
        memory.decay_rate = config.decay_rate;
        memory.initial_strength = config.initial_strength;
        Ok(memory)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryConfig;

    fn test_config() -> MemoryConfig {
        MemoryConfig {
            active_capacity: 5,
            consolidation_threshold: 2.0,
            decay_rate: 0.1,
            initial_strength: 1.0,
        }
    }

    fn make_data(val: u8) -> Data {
        Data::new(vec![val; 4])
    }

    #[test]
    fn test_store_within_capacity() {
        let config = test_config();
        let mut mem = Memory::new(&config);
        mem.store(make_data(1));
        assert_eq!(mem.active_count(), 1);
    }

    #[test]
    fn test_store_evicts_weakest() {
        let config = MemoryConfig {
            active_capacity: 3,
            consolidation_threshold: 0.5,
            ..test_config()
        };
        let mut mem = Memory::new(&config);
        mem.store_with_priority(make_data(0), 3.0);
        mem.store_with_priority(make_data(1), 3.0);
        mem.store_with_priority(make_data(2), 3.0);
        mem.store(make_data(3));
        assert_eq!(mem.active_count(), 3);
        assert_eq!(mem.long_term_count(), 1);
        assert_ne!(mem.recall(&make_data(3).payload), None);
    }

    #[test]
    fn test_store_with_priority() {
        let config = test_config();
        let mut mem = Memory::new(&config);
        mem.store_with_priority(make_data(42), 5.0);
        assert_eq!(mem.active_count(), 1);
    }

    #[test]
    fn test_consolidate() {
        let config = test_config();
        let mut mem = Memory::new(&config);
        mem.store_with_priority(make_data(0), 3.0);
        mem.store_with_priority(make_data(1), 1.0);
        mem.consolidate();
        assert_eq!(mem.long_term_count(), 1);
        assert_eq!(mem.active_count(), 1);
    }

    #[test]
    fn test_decay_retains_above_zero() {
        let config = MemoryConfig {
            decay_rate: 0.9,
            initial_strength: 1.0,
            ..test_config()
        };
        let mut mem = Memory::new(&config);
        mem.store(make_data(1));
        mem.decay();
        assert_eq!(mem.active_count(), 1);
    }

    #[test]
    fn test_decay_removes_zero_strength() {
        let config = MemoryConfig {
            decay_rate: 10.0,
            initial_strength: 1.0,
            ..test_config()
        };
        let mut mem = Memory::new(&config);
        mem.store(make_data(1));
        mem.decay();
        assert_eq!(mem.active_count(), 0);
    }

    #[test]
    fn test_recall_active() {
        let config = test_config();
        let mut mem = Memory::new(&config);
        let data = make_data(42);
        mem.store(data.clone());
        let recalled = mem.recall(&data.payload);
        assert!(recalled.is_some());
        assert_eq!(recalled.unwrap().payload, data.payload);
    }

    #[test]
    fn test_recall_long_term_promotes() {
        let config = test_config();
        let mut mem = Memory::new(&config);
        mem.store_with_priority(make_data(99), 3.0);
        mem.consolidate();
        assert_eq!(mem.long_term_count(), 1);
        let recalled = mem.recall(&[99u8; 4]);
        assert!(recalled.is_some());
        assert_eq!(mem.active_count(), 1);
        assert_eq!(mem.long_term_count(), 0);
    }

    #[test]
    fn test_recall_nonexistent() {
        let config = test_config();
        let mut mem = Memory::new(&config);
        assert!(mem.recall(&[255u8; 4]).is_none());
    }

    #[test]
    fn test_recall_by_tag() {
        let config = test_config();
        let mut mem = Memory::new(&config);
        mem.store(Data::new(vec![1]).tag("test"));
        mem.store(Data::new(vec![2]).tag("other"));
        let results = mem.recall_by_tag("test");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_empty_memory() {
        let config = test_config();
        let mem = Memory::new(&config);
        assert_eq!(mem.active_count(), 0);
        assert_eq!(mem.long_term_count(), 0);
        assert_eq!(mem.total_size(), 0);
    }

    #[test]
    fn test_serialize_roundtrip() {
        let config = test_config();
        let mut mem = Memory::new(&config);
        mem.store(make_data(7));
        mem.store_with_priority(make_data(8), 5.0);
        let json = serde_json::to_string(&mem).unwrap();
        let loaded = serde_json::from_str::<Memory>(&json).unwrap();
        assert_eq!(loaded.active.len(), mem.active.len());
    }

    #[test]
    fn test_recall_by_tag_empty() {
        let config = test_config();
        let mut mem = Memory::new(&config);
        let results = mem.recall_by_tag("nonexistent");
        assert!(results.is_empty());
    }
}
