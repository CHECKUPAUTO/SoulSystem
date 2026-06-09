//! Cross-process broadcast bus backed by shared memory.
//!
//! Uses a ring buffer per subscriber slot. A single writer (the bus owner)
//! publishes messages, and multiple reader processes consume them independently.
//!
//! Architecture:
//! ```text
//!   Writer (Orchestrator)
//!     |
//!     v
//!   [slot 0: ring buffer] --> Reader A (Guardian)
//!   [slot 1: ring buffer] --> Reader B (Inference)
//!   [slot 2: ring buffer] --> Reader C (Memory)
//!   ...
//! ```

use crate::ring_buffer::{RingError, ShmRingBuffer};
use crate::shm::ShmRegion;
use anyhow::{Context, Result};
use std::os::unix::io::OwnedFd;
use std::sync::atomic::{AtomicU64, Ordering};

/// Maximum number of subscriber slots.
pub const MAX_SUBSCRIBERS: usize = 16;

/// Default ring buffer data size per slot (256 KB).
pub const DEFAULT_SLOT_SIZE: usize = 256 * 1024;

/// Ring buffer header overhead per slot.
const RING_HEADER_OVERHEAD: usize = 64;

/// Layout of the shared memory bus region:
/// ```text
/// [bus_header: 128 bytes]
/// [slot 0: slot_header(32) + ring_header(64) + ring_data(SLOT_SIZE)]
/// [slot 1: slot_header(32) + ring_header(64) + ring_data(SLOT_SIZE)]
/// ...
/// ```
const BUS_HEADER_SIZE: usize = 128;
const SLOT_HEADER_SIZE: usize = 32;

/// Bytes per slot: slot_header + ring_buffer_total(header + data)
fn slot_stride(slot_size: usize) -> usize {
    SLOT_HEADER_SIZE + RING_HEADER_OVERHEAD + slot_size
}

/// Bus header in shared memory.
#[repr(C)]
struct BusHeader {
    magic: u64,
    version: u64,
    slot_size: u64,
    active_mask: AtomicU64, // bitmask of active subscriber slots
    sequence: AtomicU64,    // global message sequence counter
    _reserved: [u8; 64],
}

const BUS_MAGIC: u64 = 0x534F554C5F425553; // "SOUL_BUS"

/// A slot header for each subscriber.
#[repr(C)]
struct SlotHeader {
    active: u64,      // 1 = active, 0 = inactive
    reader_seq: u64,  // last sequence number read by this slot
    _reserved: [u8; 16],
}

/// A cross-process broadcast bus backed by shared memory.
pub struct ShmBus {
    region: ShmRegion,
    slot_size: usize,
}

unsafe impl Send for ShmBus {}
unsafe impl Sync for ShmBus {}

impl ShmBus {
    /// Create a new shared memory bus.
    ///
    /// `max_slots` is the number of subscriber slots (up to 16).
    /// `slot_size` is the ring buffer data size per slot.
    pub fn create(max_slots: usize, slot_size: usize) -> Result<Self> {
        assert!(max_slots <= MAX_SUBSCRIBERS);

        let total = BUS_HEADER_SIZE + max_slots * slot_stride(slot_size);

        let region = ShmRegion::create("soullink_bus", total)
            .context("failed to create bus shared memory")?;

        // Initialize bus header
        let header = region.as_mut_ptr() as *mut BusHeader;
        unsafe {
            (*header).magic = BUS_MAGIC;
            (*header).version = 1;
            (*header).slot_size = slot_size as u64;
            (*header).active_mask.store(0, Ordering::Relaxed);
            (*header).sequence.store(0, Ordering::Relaxed);
        }

        // Initialize slot headers
        let base = unsafe { region.as_mut_ptr().add(BUS_HEADER_SIZE) };
        for i in 0..max_slots {
            let slot = unsafe { base.add(i * slot_stride(slot_size)) as *mut SlotHeader };
            unsafe {
                (*slot).active = 0;
                (*slot).reader_seq = 0;
            }
        }

        Ok(Self { region, slot_size })
    }

    /// Open an existing bus from a received mmap fd.
    pub unsafe fn open(fd: OwnedFd, total_size: usize) -> Result<Self> {
        let region = ShmRegion::from_fd(fd, total_size)
            .context("failed to open bus shared memory")?;

        let header = region.as_mut_ptr() as *const BusHeader;
        if (*header).magic != BUS_MAGIC {
            anyhow::bail!("invalid bus magic");
        }

        let slot_size = (*header).slot_size as usize;

        Ok(Self { region, slot_size })
    }

    fn slot_base_ptr(&self, slot: usize) -> *mut u8 {
        unsafe {
            self.region
                .as_mut_ptr()
                .add(BUS_HEADER_SIZE + slot * slot_stride(self.slot_size))
        }
    }

    /// Subscribe a new slot. Returns the slot index.
    pub fn subscribe(&self) -> Result<usize> {
        let header = self.region.as_mut_ptr() as *mut BusHeader;

        loop {
            let mask = unsafe { (*header).active_mask.load(Ordering::Acquire) };
            let idx = mask.trailing_ones() as usize;

            if idx >= MAX_SUBSCRIBERS {
                anyhow::bail!("no more subscriber slots");
            }

            let new_mask = mask | (1u64 << idx);
            if unsafe {
                (*header)
                    .active_mask
                    .compare_exchange_weak(mask, new_mask, Ordering::AcqRel, Ordering::Relaxed)
            }
            .is_ok()
            {
                // Initialize slot header
                let slot_ptr = self.slot_base_ptr(idx) as *mut SlotHeader;
                unsafe {
                    (*slot_ptr).active = 1;
                    (*slot_ptr).reader_seq = 0;
                }

                // Initialize the ring buffer for this slot
                let ring_base = unsafe { self.slot_base_ptr(idx).add(SLOT_HEADER_SIZE) };
                let ring_total = RING_HEADER_OVERHEAD + self.slot_size;
                ShmRingBuffer::create(ring_base, ring_total, self.slot_size);

                return Ok(idx);
            }
        }
    }

    /// Unsubscribe a slot.
    pub fn unsubscribe(&self, slot: usize) {
        let header = self.region.as_mut_ptr() as *mut BusHeader;
        unsafe {
            let mask = (*header).active_mask.load(Ordering::Relaxed);
            (*header)
                .active_mask
                .store(mask & !(1u64 << slot), Ordering::Relaxed);
        }

        let slot_ptr = self.slot_base_ptr(slot) as *mut SlotHeader;
        unsafe {
            (*slot_ptr).active = 0;
        }
    }

    fn ring_buffer_for_slot(&self, slot: usize) -> ShmRingBuffer {
        let base = self.slot_base_ptr(slot);
        let ring_ptr = unsafe { base.add(SLOT_HEADER_SIZE) };
        let total = RING_HEADER_OVERHEAD + self.slot_size;
        unsafe { ShmRingBuffer::open(ring_ptr, total) }
    }

    /// Publish a message to all active subscriber slots.
    pub fn publish(&self, data: &[u8]) -> Result<usize> {
        let header = self.region.as_mut_ptr() as *mut BusHeader;
        let mask = unsafe { (*header).active_mask.load(Ordering::Acquire) };

        let mut delivered = 0;

        for slot in 0..MAX_SUBSCRIBERS {
            if mask & (1u64 << slot) == 0 {
                continue;
            }

            let slot_header = self.slot_base_ptr(slot) as *const SlotHeader;
            unsafe {
                if (*slot_header).active == 0 {
                    continue;
                }
            }

            let ring = self.ring_buffer_for_slot(slot);

            if ring.push(data).is_ok() {
                delivered += 1;
            }
        }

        // Increment global sequence
        unsafe {
            (*header).sequence.fetch_add(1, Ordering::Relaxed);
        }

        Ok(delivered)
    }

    /// Consume a message from a specific slot (consumer side).
    pub fn consume(&self, slot: usize) -> Result<Option<Vec<u8>>> {
        let ring = self.ring_buffer_for_slot(slot);

        ring.pop().map_err(|e| match e {
            RingError::Corrupted => anyhow::anyhow!("ring buffer corrupted in slot {}", slot),
            RingError::MessageTooLarge => anyhow::anyhow!("message too large in slot {}", slot),
        })
    }

    /// Get the global sequence counter.
    pub fn sequence(&self) -> u64 {
        let header = self.region.as_ptr() as *const BusHeader;
        unsafe { (*header).sequence.load(Ordering::Relaxed) }
    }

    /// Get the raw shared memory region (for passing via UDS).
    pub fn region(&self) -> &ShmRegion {
        &self.region
    }

    /// Total size of the bus region.
    pub fn total_size(&self) -> usize {
        self.region.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_publish() {
        let bus = ShmBus::create(4, 4096).unwrap();

        let slot = bus.subscribe().unwrap();
        assert_eq!(slot, 0);

        bus.publish(b"hello").unwrap();
        let msg = bus.consume(slot).unwrap().unwrap();
        assert_eq!(msg, b"hello");

        bus.unsubscribe(slot);
    }

    #[test]
    fn test_multiple_subscribers() {
        let bus = ShmBus::create(4, 4096).unwrap();

        let s0 = bus.subscribe().unwrap();
        let s1 = bus.subscribe().unwrap();

        bus.publish(b"msg1").unwrap();
        bus.publish(b"msg2").unwrap();

        assert_eq!(bus.consume(s0).unwrap().unwrap(), b"msg1");
        assert_eq!(bus.consume(s0).unwrap().unwrap(), b"msg2");
        assert_eq!(bus.consume(s1).unwrap().unwrap(), b"msg1");
        assert_eq!(bus.consume(s1).unwrap().unwrap(), b"msg2");

        bus.unsubscribe(s0);
        bus.unsubscribe(s1);
    }

    #[test]
    fn test_publish_empty_when_no_subscribers() {
        let bus = ShmBus::create(4, 4096).unwrap();
        let delivered = bus.publish(b"nobody listening").unwrap();
        assert_eq!(delivered, 0);
    }

    #[test]
    fn test_subscribe_unsubscribe_subscribe() {
        let bus = ShmBus::create(4, 4096).unwrap();

        let s0 = bus.subscribe().unwrap();
        bus.unsubscribe(s0);

        let s1 = bus.subscribe().unwrap();
        // Should reuse slot 0
        assert_eq!(s1, 0);
        bus.unsubscribe(s1);
    }
}
