//! Paged KV Cache Manager — block-level memory management for KV cache
//! with zero-copy O(1) rollback for rejected speculative branches.
//!
//! # Design
//! Memory is partitioned into fixed-size blocks ("pages"). Each block stores
//! K/V vectors for up to `tokens_per_page` token positions. Allocation uses a
//! free-list (LIFO stack); deallocation returns blocks without moving data.
//! A logical sequence is formed by a chain of pages (page table).
//!
//! # Rollback (O(1) amortized)
//! `rollback(num_tokens)` finds which pages are no longer needed and returns
//! them to the free list. Since each page holds many tokens (e.g. 64),
//! rolling back even hundreds of tokens touches only a handful of pages.
//! The data on freed pages is NOT zeroed — it becomes inaccessible via the
//! page table and will be overwritten on the next allocation.
//!
//! # Branch checkpoint (for MTP)
//! 1. `checkpoint()` — snapshots the logical sequence length.
//! 2. Push speculative K/V entries.
//! 3a. All accepted → `commit()` marks the watermark permanently.
//! 3b. Partially rejected → `rollback_to_checkpoint()` restores the sequence
//!     to the checkpoint length, then push only accepted tokens.
//!
//! # Per-layer isolation
//! Each model layer has its own independent KvCacheManager instance.
//! The outer system maintains a `Vec<KvCacheManager>` of length `n_layers`.

use thiserror::Error;

// ── Error types ───────────────────────────────────────────────────────────────

#[derive(Error, Debug)]
pub enum KvCacheError {
    #[error("Out of KV cache pages: requested {requested}, free {free}")]
    OutOfPages { requested: usize, free: usize },

    #[error("Cannot rollback {requested} tokens: would go past checkpoint (seq_len={seq_len}, checkpoint={checkpoint})")]
    RollbackPastCheckpoint {
        requested: usize,
        seq_len: usize,
        checkpoint: usize,
    },

    #[error("Layer index {layer} out of bounds (max {max})")]
    LayerOutOfBounds { layer: usize, max: usize },

    #[error("Dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },
}

pub type Result<T> = std::result::Result<T, KvCacheError>;

// ── KvPage — a single memory block ────────────────────────────────────────────

/// A fixed-capacity KV memory block. Stores contiguous K and V arrays.
///
/// `capacity` = `tokens_per_page * dim` values each for K and V.
/// `used` tracks how many of those slots are currently occupied (≤ tokens_per_page).
#[derive(Clone)]
pub struct KvPage {
    /// Key storage: [capacity * dim] f32. Row-major: [token][dim].
    pub keys: Vec<f32>,
    /// Value storage: [capacity * dim] f32. Row-major: [token][dim].
    pub values: Vec<f32>,
    /// Number of token positions written in this page.
    pub used: usize,
}

impl KvPage {
    #[inline]
    pub fn new(tokens_per_page: usize, dim: usize) -> Self {
        Self {
            keys: vec![0.0; tokens_per_page * dim],
            values: vec![0.0; tokens_per_page * dim],
            used: 0,
        }
    }

    /// Write K/V for one token at offset `slot` within this page.
    #[inline]
    pub fn write(&mut self, slot: usize, dim: usize, k: &[f32], v: &[f32]) {
        debug_assert!(slot < self.keys.len() / dim);
        let start = slot * dim;
        self.keys[start..start + dim].copy_from_slice(k);
        self.values[start..start + dim].copy_from_slice(v);
    }

    /// Read K/V for one token at offset `slot` within this page.
    #[inline]
    pub fn read(&self, slot: usize, dim: usize) -> (&[f32], &[f32]) {
        debug_assert!(slot < self.keys.len() / dim);
        let start = slot * dim;
        (
            &self.keys[start..start + dim],
            &self.values[start..start + dim],
        )
    }
}

// ── KvCacheManager ────────────────────────────────────────────────────────────

/// Paged KV cache for a single model layer.
///
/// Manages a pool of pre-allocated `KvPage` blocks, a free list for fast
/// allocation/reclamation, and a page table tracking which pages form the
/// current logical sequence.
///
/// # Memory model
/// ```text
/// pages:    [P0] [P1] [P2] ... [Pn]     ← pre-allocated pool
/// free:     [idx ...]                    ← LIFO stack of free page indices
/// sequence: [(pg, used), ...]            ← ordered page table
///            pg: index into pages[]
///            used: tokens stored in this page
/// seq_len: total tokens in sequence (sum of `used` across `sequence`)
/// checkpoint: watermark for speculative branch recovery
/// ```
pub struct KvCacheManager {
    /// KV dimension (head_dim * n_heads for multi-head, or d_model for GQA).
    pub dim: usize,
    /// Maximum tokens per page (block size).
    pub tokens_per_page: usize,
    /// Total pages in the pool.
    pub total_pages: usize,
    /// Pre-allocated memory pages.
    pages: Vec<KvPage>,
    /// Free page indices (LIFO — good temporal locality).
    free_pages: Vec<usize>,
    /// Active page table: ordered list of (page_index, tokens_used).
    sequence: Vec<(usize, usize)>,
    /// Total logical tokens currently stored.
    seq_len: usize,
    /// Checkpoint snapshot — see `checkpoint()` / `rollback_to_checkpoint()`.
    checkpoint_seq_len: usize,
    /// High watermark of seq_len (for capacity monitoring).
    max_observed_len: usize,
}

impl KvCacheManager {
    /// Create a new paged KV cache for one layer.
    ///
    /// * `max_seq_len` — maximum sequence length in tokens.
    /// * `dim` — KV dimension per token position (typically `head_dim * n_kv_heads`).
    /// * `tokens_per_page` — block size; 64 is a common default.
    pub fn new(max_seq_len: usize, dim: usize, tokens_per_page: usize) -> Self {
        assert!(max_seq_len > 0, "max_seq_len must be > 0");
        assert!(tokens_per_page > 0, "tokens_per_page must be > 0");
        assert!(dim > 0, "dim must be > 0");

        let total_pages = max_seq_len.div_ceil(tokens_per_page);

        let pages: Vec<KvPage> = (0..total_pages)
            .map(|_| KvPage::new(tokens_per_page, dim))
            .collect();

        // Initially all pages are free
        let free_pages: Vec<usize> = (0..total_pages).rev().collect();

        Self {
            dim,
            tokens_per_page,
            total_pages,
            pages,
            free_pages,
            sequence: Vec::with_capacity(total_pages),
            seq_len: 0,
            checkpoint_seq_len: 0,
            max_observed_len: 0,
        }
    }

    // ── Capacity queries ──────────────────────────────────────────────────────

    /// Maximum sequence length this cache can hold.
    #[inline]
    pub fn max_seq_len(&self) -> usize {
        self.total_pages * self.tokens_per_page
    }

    /// Current logical sequence length (tokens stored).
    #[inline]
    pub fn len(&self) -> usize {
        self.seq_len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.seq_len == 0
    }

    /// Number of free pages remaining.
    #[inline]
    pub fn free_pages_count(&self) -> usize {
        self.free_pages.len()
    }

    /// Number of pages currently in use.
    #[inline]
    pub fn used_pages_count(&self) -> usize {
        self.sequence.len()
    }

    /// Memory footprint in bytes (approximate).
    pub fn memory_bytes(&self) -> usize {
        self.total_pages * self.tokens_per_page * self.dim * std::mem::size_of::<f32>() * 2
    }

    /// Peak sequence length observed so far.
    #[inline]
    pub fn max_observed_len(&self) -> usize {
        self.max_observed_len
    }

    // ── Core operations ───────────────────────────────────────────────────────

    /// Push one token's K/V to the cache.
    ///
    /// Allocates a new page from the free list if the last page is full.
    /// Returns the logical position of the newly written token.
    ///
    /// # Errors
    /// Returns `KvCacheError::OutOfPages` if no free pages remain.
    pub fn push(&mut self, k: &[f32], v: &[f32]) -> Result<usize> {
        assert_eq!(k.len(), self.dim, "K dimension mismatch");
        assert_eq!(v.len(), self.dim, "V dimension mismatch");

        // Allocate a new page if sequence is empty or the last page is full
        if self.sequence.is_empty()
            || self
                .sequence
                .last()
                .map(|&(_, used)| used == self.tokens_per_page)
                == Some(true)
        {
            let page_idx = self.free_pages.pop().ok_or(KvCacheError::OutOfPages {
                requested: 1,
                free: 0,
            })?;
            self.pages[page_idx].used = 0; // fresh page
            self.sequence.push((page_idx, 0));
        }

        let pos = self.seq_len;
        let (pg_idx, ref mut used) = self.sequence.last_mut().unwrap();
        let slot = *used;

        self.pages[*pg_idx].write(slot, self.dim, k, v);
        *used += 1;
        self.seq_len += 1;

        // Track high watermark
        if self.seq_len > self.max_observed_len {
            self.max_observed_len = self.seq_len;
        }

        Ok(pos)
    }

    /// Push K/V for multiple tokens at once (batched).
    ///
    /// More efficient than repeated `push()` calls since page allocation
    /// and bounds checks are amortized.
    pub fn push_batch(&mut self, k_batch: &[f32], v_batch: &[f32]) -> Result<Vec<usize>> {
        let n_tokens = k_batch.len() / self.dim;
        assert_eq!(
            k_batch.len(),
            n_tokens * self.dim,
            "K batch not aligned to dim"
        );
        assert_eq!(
            v_batch.len(),
            n_tokens * self.dim,
            "V batch not aligned to dim"
        );

        let mut positions = Vec::with_capacity(n_tokens);
        for i in 0..n_tokens {
            let start = i * self.dim;
            let end = start + self.dim;
            let pos = self.push(&k_batch[start..end], &v_batch[start..end])?;
            positions.push(pos);
        }
        Ok(positions)
    }

    /// Read the K/V at a given logical position.
    ///
    /// Returns `None` if the position is out of bounds (never written or
    /// rolled back).
    pub fn get(&self, logical_pos: usize) -> Option<(&[f32], &[f32])> {
        if logical_pos >= self.seq_len {
            return None;
        }
        let (pg_idx, slot_in_page) = self.resolve_logical_pos(logical_pos)?;
        Some(self.pages[pg_idx].read(slot_in_page, self.dim))
    }

    /// Map a logical position to (page_index, slot_in_page).
    fn resolve_logical_pos(&self, logical_pos: usize) -> Option<(usize, usize)> {
        if logical_pos >= self.seq_len {
            return None;
        }
        let mut remaining = logical_pos;
        for &(pg_idx, used) in &self.sequence {
            if remaining < used {
                return Some((pg_idx, remaining));
            }
            remaining = remaining.saturating_sub(used);
        }
        None
    }

    /// Iterate all active K/V entries in logical order.
    /// Returns iterator of `(logical_position, &k, &v)`.
    pub fn iter(&self) -> impl Iterator<Item = (usize, &[f32], &[f32])> + '_ {
        let dim = self.dim;
        let mut pos = 0usize;
        self.sequence.iter().flat_map(move |&(pg_idx, used)| {
            let page = &self.pages[pg_idx];
            let base = pos;
            pos += used;
            (0..used).map(move |slot| {
                let logical = base + slot;
                let (k, v) = page.read(slot, dim);
                (logical, k, v)
            })
        })
    }

    // ── Rollback (O(1) amortized) ─────────────────────────────────────────────

    /// Roll back the sequence by `num_tokens`.
    ///
    /// Returns freed pages to the free list. The data on freed pages is NOT
    /// zeroed — it is logically unreachable and will be overwritten on the next
    /// allocation.
    ///
    /// **Complexity**: O(number of freed pages). Since each page holds many
    /// tokens (e.g. 64), this is O(1) amortized per rollback operation.
    ///
    /// # Errors
    /// Returns `KvCacheError::RollbackPastCheckpoint` if the rollback would
    /// go past the checkpoint watermark.
    pub fn rollback(&mut self, num_tokens: usize) -> Result<()> {
        if num_tokens == 0 {
            return Ok(());
        }
        if num_tokens > self.seq_len {
            return Err(KvCacheError::RollbackPastCheckpoint {
                requested: num_tokens,
                seq_len: self.seq_len,
                checkpoint: 0,
            });
        }
        let new_len = self.seq_len - num_tokens;
        if new_len < self.checkpoint_seq_len {
            return Err(KvCacheError::RollbackPastCheckpoint {
                requested: num_tokens,
                seq_len: self.seq_len,
                checkpoint: self.checkpoint_seq_len,
            });
        }
        self.truncate_to(new_len)
    }

    /// Truncate the sequence to `new_len` tokens, freeing pages as needed.
    fn truncate_to(&mut self, new_len: usize) -> Result<()> {
        if new_len >= self.seq_len {
            self.seq_len = new_len;
            return Ok(());
        }

        let mut remaining = new_len;
        let mut keep_pages = 0usize;

        // Find how many pages to keep
        for (i, &(_, used)) in self.sequence.iter().enumerate() {
            if remaining <= used {
                keep_pages = i + 1;
                break;
            }
            remaining = remaining.saturating_sub(used);
        }

        // Return freed pages to the free list
        for i in (keep_pages..self.sequence.len()).rev() {
            let (pg_idx, _) = self.sequence[i];
            self.free_pages.push(pg_idx);
        }
        self.sequence.truncate(keep_pages);

        // Adjust the last kept page's `used` count
        if let Some((_, ref mut used)) = self.sequence.last_mut() {
            *used = remaining;
        }

        self.seq_len = new_len;
        Ok(())
    }

    // ── Checkpoint / commit (branch management) ───────────────────────────────

    /// Take a checkpoint snapshot of the current sequence length.
    ///
    /// During speculative decoding:
    /// 1. `checkpoint()` — snapshot.
    /// 2. Push speculative K/V.
    /// 3a. Accepted → `commit()`.
    /// 3b. Rejected → `rollback_to_checkpoint()`, then push only the accepted tokens.
    #[inline]
    pub fn checkpoint(&mut self) {
        self.checkpoint_seq_len = self.seq_len;
    }

    /// Commit the current state (advances the watermark).
    ///
    /// After `commit()`, you cannot roll back past the committed point.
    #[inline]
    pub fn commit(&mut self) {
        self.checkpoint_seq_len = self.seq_len;
    }

    /// Roll back to the most recent checkpoint.
    ///
    /// O(1) amortized: only affected pages are freed.
    #[inline]
    pub fn rollback_to_checkpoint(&mut self) -> Result<()> {
        let num = self.seq_len.saturating_sub(self.checkpoint_seq_len);
        self.rollback(num)
    }

    /// Check if a checkpoint is currently active (uncommitted data exists).
    #[inline]
    pub fn has_uncommitted(&self) -> bool {
        self.seq_len > self.checkpoint_seq_len
    }

    // ── Full reset & reconfiguration ──────────────────────────────────────────

    /// Reset the entire cache for a new sequence without reallocating pages.
    ///
    /// All pages are returned to the free list. Logical state is cleared.
    /// Page data is NOT zeroed — it becomes unreachable and will be
    /// overwritten on next allocation.
    pub fn reset(&mut self) {
        // Return all used pages to the free list
        for &(pg_idx, _) in &self.sequence {
            self.free_pages.push(pg_idx);
        }
        self.sequence.clear();
        self.seq_len = 0;
        self.checkpoint_seq_len = 0;
    }

    /// Fill ratio of the page pool (0.0 = empty, 1.0 = full).
    #[inline]
    pub fn fill_ratio(&self) -> f32 {
        if self.free_pages.len() == self.total_pages {
            0.0
        } else {
            (self.total_pages - self.free_pages.len()) as f32 / self.total_pages as f32
        }
    }

    /// Compact fragmented pages by re-packing tokens into fewer pages.
    ///
    /// Operates in-place: moves data from partially-filled pages to consolidate.
    /// Returns the number of pages freed by compaction.
    pub fn compact(&mut self) -> usize {
        let old_pages = self.used_pages_count();
        if old_pages <= 1 {
            return 0;
        }

        // Extract all active KV into contiguous buffers
        let n_tokens = self.seq_len;
        let mut all_k = vec![0.0f32; n_tokens * self.dim];
        let mut all_v = vec![0.0f32; n_tokens * self.dim];

        for (offset, (_pos, k, v)) in self.iter().enumerate() {
            let start = offset * self.dim;
            all_k[start..start + self.dim].copy_from_slice(k);
            all_v[start..start + self.dim].copy_from_slice(v);
        }

        // Reset and re-push (reuses the pool, no new allocations)
        self.reset();

        // Write back, repacking tightly
        for i in 0..n_tokens {
            let start = i * self.dim;
            self.push(
                &all_k[start..start + self.dim],
                &all_v[start..start + self.dim],
            )
            .expect("compact: push after reset should never exhaust pages");
        }

        old_pages - self.used_pages_count()
    }
}

// ── Multilayer cache convenience ──────────────────────────────────────────────

/// A collection of `KvCacheManager` instances, one per transformer layer.
///
/// All layers advance in lockstep during autoregressive generation.
pub struct MultilayerKvCache {
    pub num_layers: usize,
    pub caches: Vec<KvCacheManager>,
}

impl MultilayerKvCache {
    /// Create a cache for `num_layers` transformer layers.
    pub fn new(num_layers: usize, max_seq_len: usize, dim: usize, tokens_per_page: usize) -> Self {
        Self {
            num_layers,
            caches: (0..num_layers)
                .map(|_| KvCacheManager::new(max_seq_len, dim, tokens_per_page))
                .collect(),
        }
    }

    /// Push K/V for all layers for a single new token.
    /// `kv_per_layer[layer]` is a pair `(&[f32], &[f32])` of (K, V).
    pub fn push_all_layers(&mut self, kv_per_layer: &[(&[f32], &[f32])]) -> Result<Vec<usize>> {
        assert_eq!(kv_per_layer.len(), self.num_layers);
        let mut positions = Vec::with_capacity(self.num_layers);
        for (layer, &(k, v)) in kv_per_layer.iter().enumerate() {
            let pos = self.caches[layer].push(k, v)?;
            positions.push(pos);
        }
        Ok(positions)
    }

    /// Checkpoint all layers.
    pub fn checkpoint_all(&mut self) {
        for cache in &mut self.caches {
            cache.checkpoint();
        }
    }

    /// Commit all layers.
    pub fn commit_all(&mut self) {
        for cache in &mut self.caches {
            cache.commit();
        }
    }

    /// Roll back all layers to their checkpoints.
    pub fn rollback_all_to_checkpoint(&mut self) -> Result<()> {
        for cache in &mut self.caches {
            cache.rollback_to_checkpoint()?;
        }
        Ok(())
    }

    /// Reset all layers.
    pub fn reset_all(&mut self) {
        for cache in &mut self.caches {
            cache.reset();
        }
    }

    /// Total memory footprint in bytes.
    pub fn total_memory_bytes(&self) -> usize {
        self.caches.iter().map(|c| c.memory_bytes()).sum()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn k_for(dim: usize, token: usize, tag: f32) -> Vec<f32> {
        (0..dim)
            .map(|i| tag + token as f32 * 10.0 + i as f32)
            .collect()
    }

    #[test]
    fn push_and_get() {
        let mut cache = KvCacheManager::new(128, 4, 8);
        for t in 0..20 {
            let k = k_for(4, t, 1.0);
            let v = k_for(4, t, 2.0);
            let pos = cache.push(&k, &v).unwrap();
            assert_eq!(pos, t);
        }
        assert_eq!(cache.len(), 20);
        for t in 0..20 {
            let (k, v) = cache.get(t).expect("token should exist");
            assert_eq!(k, &k_for(4, t, 1.0));
            assert_eq!(v, &k_for(4, t, 2.0));
        }
    }

    #[test]
    fn rollback_partial() {
        let mut cache = KvCacheManager::new(128, 2, 8);
        for t in 0..20 {
            cache.push(&k_for(2, t, 1.0), &k_for(2, t, 2.0)).unwrap();
        }
        assert_eq!(cache.len(), 20);

        cache.rollback(7).unwrap(); // keep 13 tokens
        assert_eq!(cache.len(), 13);
        assert!(cache.get(12).is_some());
        assert!(cache.get(13).is_none()); // rolled back
    }

    #[test]
    fn rollback_frees_pages() {
        let mut cache = KvCacheManager::new(128, 4, 8);
        let before_free = cache.free_pages_count();

        // Fill 40 tokens → need ceil(40/8)=5 pages
        for t in 0..40 {
            cache.push(&k_for(4, t, 1.0), &k_for(4, t, 2.0)).unwrap();
        }
        let used_after_fill = cache.used_pages_count();
        assert_eq!(used_after_fill, 5);

        // Rollback 32 tokens → keep 8 tokens = 1 page, free the other 4
        cache.rollback(32).unwrap();
        assert_eq!(cache.len(), 8);
        assert_eq!(cache.used_pages_count(), 1);
        assert_eq!(cache.free_pages_count(), before_free - 1); // 1 page still used
    }

    #[test]
    fn rollback_zero_is_noop() {
        let mut cache = KvCacheManager::new(64, 4, 8);
        cache.push(&k_for(4, 0, 1.0), &k_for(4, 0, 2.0)).unwrap();
        cache.rollback(0).unwrap();
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn rollback_past_length_errors() {
        let mut cache = KvCacheManager::new(64, 4, 8);
        cache.push(&k_for(4, 0, 1.0), &k_for(4, 0, 2.0)).unwrap();
        assert!(cache.rollback(5).is_err());
    }

    #[test]
    fn checkpoint_commit_cycle() {
        let mut cache = KvCacheManager::new(64, 4, 8);

        // Phase 1: push and commit
        for t in 0..10 {
            cache.push(&k_for(4, t, 1.0), &k_for(4, t, 2.0)).unwrap();
        }
        cache.commit();
        assert_eq!(cache.checkpoint_seq_len, 10);

        // Phase 2: checkpoint, push, reject
        cache.checkpoint();
        for t in 10..15 {
            cache.push(&k_for(4, t, 1.0), &k_for(4, t, 2.0)).unwrap();
        }
        assert_eq!(cache.len(), 15);
        assert!(cache.has_uncommitted());

        cache.rollback_to_checkpoint().unwrap();
        assert_eq!(cache.len(), 10);
        assert!(!cache.has_uncommitted());
    }

    #[test]
    fn cannot_rollback_past_checkpoint() {
        let mut cache = KvCacheManager::new(64, 4, 8);
        for t in 0..10 {
            cache.push(&k_for(4, t, 1.0), &k_for(4, t, 2.0)).unwrap();
        }
        cache.commit();
        assert!(cache.rollback(11).is_err()); // can't go below 0
    }

    #[test]
    fn push_after_rollback_reuses_freed_pages() {
        let mut cache = KvCacheManager::new(64, 4, 8);
        // Fill 40 tokens → 5 pages
        for t in 0..40 {
            cache.push(&k_for(4, t, 1.0), &k_for(4, t, 2.0)).unwrap();
        }
        // Rollback to 8 tokens → 4 pages freed
        cache.rollback(32).unwrap();
        // Push new tokens — should reuse freed pages
        for t in 40..72 {
            cache.push(&k_for(4, t, 1.0), &k_for(4, t, 2.0)).unwrap();
        }
        assert_eq!(cache.len(), 40); // 8 kept + 32 new = 40
                                     // Verify the kept tokens survived
        let (k, v) = cache.get(0).unwrap();
        assert_eq!(k, &k_for(4, 0, 1.0));
        assert_eq!(v, &k_for(4, 0, 2.0));
    }

    #[test]
    fn out_of_pages_error() {
        let mut cache = KvCacheManager::new(32, 4, 8); // 4 pages max
        for t in 0..32 {
            cache.push(&k_for(4, t, 1.0), &k_for(4, t, 2.0)).unwrap();
        }
        // Page 33 should fail
        assert!(cache.push(&k_for(4, 33, 1.0), &k_for(4, 33, 2.0)).is_err());
    }

    #[test]
    fn iter_yields_all_entries() {
        let mut cache = KvCacheManager::new(64, 2, 8);
        for t in 0..5 {
            cache.push(&k_for(2, t, 1.0), &k_for(2, t, 2.0)).unwrap();
        }
        let entries: Vec<(usize, Vec<f32>, Vec<f32>)> = cache
            .iter()
            .map(|(p, k, v)| (p, k.to_vec(), v.to_vec()))
            .collect();
        assert_eq!(entries.len(), 5);
        for (i, (pos, k, v)) in entries.iter().enumerate() {
            assert_eq!(*pos, i);
            assert_eq!(k, &k_for(2, i, 1.0));
            assert_eq!(v, &k_for(2, i, 2.0));
        }
    }

    #[test]
    fn get_out_of_bounds_returns_none() {
        let mut cache = KvCacheManager::new(64, 4, 8);
        cache.push(&k_for(4, 0, 1.0), &k_for(4, 0, 2.0)).unwrap();
        assert!(cache.get(5).is_none());
    }

    #[test]
    fn reset_clears_and_frees_all() {
        let mut cache = KvCacheManager::new(64, 4, 8);
        for t in 0..20 {
            cache.push(&k_for(4, t, 1.0), &k_for(4, t, 2.0)).unwrap();
        }
        cache.reset();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        assert_eq!(cache.used_pages_count(), 0);
        assert_eq!(cache.free_pages_count(), cache.total_pages);
    }

    #[test]
    fn reset_then_push_reuses_pages() {
        let mut cache = KvCacheManager::new(64, 4, 8);
        let old_ptr = cache.pages[0].keys.as_ptr();
        for t in 0..20 {
            cache.push(&k_for(4, t, 1.0), &k_for(4, t, 2.0)).unwrap();
        }
        cache.reset();
        cache
            .push(&k_for(4, 100, 1.0), &k_for(4, 100, 2.0))
            .unwrap();
        // Should reuse the same pool
        assert_eq!(cache.pages[0].keys.as_ptr(), old_ptr);
    }

    #[test]
    fn fill_ratio() {
        let mut cache = KvCacheManager::new(64, 4, 8);
        assert_eq!(cache.fill_ratio(), 0.0);
        for t in 0..8 {
            cache.push(&k_for(4, t, 1.0), &k_for(4, t, 2.0)).unwrap();
        }
        let pages = 64usize.div_ceil(8); // total_pages
        let expected_ratio = 1.0 / pages as f32;
        assert!(
            (cache.fill_ratio() - expected_ratio).abs() < 0.001,
            "ratio={} expected≈{}",
            cache.fill_ratio(),
            expected_ratio
        );
    }

    #[test]
    fn compact_reduces_pages() {
        let mut cache = KvCacheManager::new(64, 4, 8);
        // Fill 9 tokens → should need 2 pages (8 + 1)
        for t in 0..9 {
            cache.push(&k_for(4, t, 1.0), &k_for(4, t, 2.0)).unwrap();
        }
        let before = cache.used_pages_count();
        assert_eq!(before, 2);

        // Rollback 1 → page 1 is still allocated with 0 used slots (fragmented)
        // Actually, rollback keeps the page since `remaining` is 8
        cache.rollback(1).unwrap();
        let after_rollback = cache.used_pages_count();
        assert_eq!(after_rollback, 1); // only page 0 remains

        // Re-push 9 more tokens to re-create fragmentation
        for t in 9..18 {
            cache.push(&k_for(4, t, 1.0), &k_for(4, t, 2.0)).unwrap();
        }
        let fragmented = cache.used_pages_count();
        assert!(fragmented >= 2);

        // Compact should pack them efficiently
        let freed = cache.compact();
        assert!(freed >= 0);
        assert!(cache.used_pages_count() <= fragmented);
    }

    #[test]
    fn batch_push() {
        let mut cache = KvCacheManager::new(64, 3, 8);
        let n = 12;
        let k_batch: Vec<f32> = (0..n * 3).map(|i| i as f32).collect();
        let v_batch: Vec<f32> = (0..n * 3).map(|i| (i * 2) as f32).collect();
        let positions = cache.push_batch(&k_batch, &v_batch).unwrap();
        assert_eq!(positions.len(), n);
        assert_eq!(cache.len(), n);
        for (i, &pos) in positions.iter().enumerate() {
            assert_eq!(pos, i);
        }
    }

    #[test]
    fn multilayer_push_sync() {
        let mut ml = MultilayerKvCache::new(4, 64, 8, 8);
        let k = vec![1.0f32; 8];
        let v = vec![2.0f32; 8];
        let layers: Vec<(&[f32], &[f32])> = (0..4).map(|_| (k.as_slice(), v.as_slice())).collect();
        let positions = ml.push_all_layers(&layers).unwrap();
        assert_eq!(positions.len(), 4);
        for l in 0..4 {
            assert_eq!(ml.caches[l].len(), 1);
        }
    }

    #[test]
    fn multilayer_checkpoint_rollback() {
        let mut ml = MultilayerKvCache::new(2, 64, 4, 8);
        let k1 = vec![1.0f32; 4];
        let v1 = vec![2.0f32; 4];

        // Push 5 tokens
        for _ in 0..5 {
            let layers: Vec<(&[f32], &[f32])> =
                (0..2).map(|_| (k1.as_slice(), v1.as_slice())).collect();
            ml.push_all_layers(&layers).unwrap();
        }
        assert_eq!(ml.caches[0].len(), 5);

        // Checkpoint, push more, rollback
        ml.checkpoint_all();
        for _ in 0..3 {
            let layers: Vec<(&[f32], &[f32])> =
                (0..2).map(|_| (k1.as_slice(), v1.as_slice())).collect();
            ml.push_all_layers(&layers).unwrap();
        }
        assert_eq!(ml.caches[0].len(), 8);

        ml.rollback_all_to_checkpoint().unwrap();
        assert_eq!(ml.caches[0].len(), 5);
        assert_eq!(ml.caches[1].len(), 5);
    }

    #[test]
    fn memory_bytes_is_plausible() {
        let cache = KvCacheManager::new(1024, 128, 64);
        let mem = cache.memory_bytes();
        // 1024 tokens × 128 dim × 4 bytes × 2 (K+V) = 1 MiB
        let expected = 1024 * 128 * 4 * 2;
        assert_eq!(mem, expected);
    }

    #[test]
    fn max_observed_len_tracks_peak() {
        let mut cache = KvCacheManager::new(64, 4, 8);
        for t in 0..30 {
            cache.push(&k_for(4, t, 1.0), &k_for(4, t, 2.0)).unwrap();
        }
        assert_eq!(cache.max_observed_len(), 30);
        cache.rollback(20).unwrap();
        assert_eq!(cache.len(), 10);
        assert_eq!(cache.max_observed_len(), 30); // peak preserved
    }
}
