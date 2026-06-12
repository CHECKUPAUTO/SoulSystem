//! Lock-free SPSC (Single-Producer, Single-Consumer) ring buffer.
//!
//! Backed by a shared memory region. Uses atomic operations for
//! head/tail pointers — no locks needed for single-writer, single-reader.
//!
//! Layout in shared memory:
//! ```text
//! [header: 64 bytes] [data: capacity bytes]
//!   head (u64)        ^
//!   tail (u64)        |
//!   capacity (u64)    |
//!   magic (u64)       |
//!   reserved (32 bytes)
//! ```

use std::sync::atomic::{AtomicU64, Ordering};

const HEADER_SIZE: usize = 64;
const MAGIC: u64 = 0x534F554C5F52494E; // "SOUL_RING"

/// Header layout in the shared memory region.
#[repr(C)]
struct RingHeader {
    head: AtomicU64,
    tail: AtomicU64,
    capacity: u64,
    magic: u64,
    _reserved: [u8; 32],
}

/// A lock-free SPSC ring buffer backed by shared memory.
pub struct ShmRingBuffer {
    ptr: *mut u8,
    #[allow(dead_code)]
    len: usize,
}

unsafe impl Send for ShmRingBuffer {}
unsafe impl Sync for ShmRingBuffer {}

impl ShmRingBuffer {
    /// Taille totale (en-tête compris) de la région partagée sous-jacente.
    pub fn region_len(&self) -> usize {
        self.len
    }

    /// Create a new ring buffer in the given shared memory region.
    ///
    /// `data_size` is the usable capacity in bytes (excluding header).
    pub fn create(ptr: *mut u8, total_size: usize, data_size: usize) -> Self {
        assert!(
            total_size >= HEADER_SIZE + data_size,
            "region too small for requested capacity"
        );

        let header = ptr as *mut RingHeader;
        unsafe {
            (*header).head.store(0, Ordering::Relaxed);
            (*header).tail.store(0, Ordering::Relaxed);
            (*header).capacity = data_size as u64;
            (*header).magic = MAGIC;
        }

        Self {
            ptr,
            len: total_size,
        }
    }

    /// Open an existing ring buffer (e.g., from a received mmap fd).
    ///
    /// # Safety
    /// `ptr` doit pointer vers une région d'au moins `len` octets contenant
    /// un ring buffer initialisé par `create` (en-tête magic valide), et
    /// rester valide pendant toute la durée de vie du buffer.
    pub unsafe fn open(ptr: *mut u8, len: usize) -> Self {
        let header = ptr as *const RingHeader;
        let magic = (*header).magic;
        assert_eq!(magic, MAGIC, "invalid ring buffer magic");

        Self { ptr, len }
    }

    fn header(&self) -> &RingHeader {
        unsafe { &*(self.ptr as *const RingHeader) }
    }

    fn data_ptr(&self) -> *mut u8 {
        unsafe { self.ptr.add(HEADER_SIZE) }
    }

    fn capacity(&self) -> usize {
        self.header().capacity as usize
    }

    /// Write a message into the ring buffer (producer side).
    ///
    /// Returns `Ok(true)` if written, `Ok(false)` if full, `Err` on overflow.
    pub fn push(&self, data: &[u8]) -> Result<bool, RingError> {
        let h = self.header();
        let cap = self.capacity();
        let head = h.head.load(Ordering::Acquire);
        let tail = h.tail.load(Ordering::Acquire);

        let used = head.wrapping_sub(tail) as usize;
        let free = cap - used;

        // 4 bytes for length prefix
        let total = 4 + data.len();
        if total > free {
            return Ok(false); // Full
        }

        if total > cap {
            return Err(RingError::MessageTooLarge);
        }

        let data_offset = (head as usize) % cap;
        let data_ptr = self.data_ptr();

        // Write length prefix (u32 LE)
        let len_bytes = (data.len() as u32).to_le_bytes();
        unsafe {
            let write_pos = data_ptr.add(data_offset);
            // Handle wrap-around for the 4-byte length prefix
            if data_offset + 4 <= cap {
                std::ptr::copy_nonoverlapping(len_bytes.as_ptr(), write_pos, 4);
                std::ptr::copy_nonoverlapping(data.as_ptr(), write_pos.add(4), data.len());
            } else {
                // Wraps around — write byte by byte
                for (i, &b) in len_bytes.iter().enumerate() {
                    *data_ptr.add((data_offset + i) % cap) = b;
                }
                for (i, &b) in data.iter().enumerate() {
                    *data_ptr.add((data_offset + 4 + i) % cap) = b;
                }
            }
        }

        h.head
            .store(head.wrapping_add(total as u64), Ordering::Release);
        Ok(true)
    }

    /// Read the next message from the ring buffer (consumer side).
    ///
    /// Returns `Ok(None)` if empty, `Ok(Some(data))` if a message was read.
    pub fn pop(&self) -> Result<Option<Vec<u8>>, RingError> {
        let h = self.header();
        let cap = self.capacity();
        let head = h.head.load(Ordering::Acquire);
        let tail = h.tail.load(Ordering::Acquire);

        if head == tail {
            return Ok(None); // Empty
        }

        let data_offset = (tail as usize) % cap;
        let data_ptr = self.data_ptr();

        // Read length prefix (u32 LE)
        let len_bytes = unsafe {
            let mut buf = [0u8; 4];
            if data_offset + 4 <= cap {
                std::ptr::copy_nonoverlapping(data_ptr.add(data_offset), buf.as_mut_ptr(), 4);
            } else {
                for (i, b) in buf.iter_mut().enumerate() {
                    *b = *data_ptr.add((data_offset + i) % cap);
                }
            }
            buf
        };

        let msg_len = u32::from_le_bytes(len_bytes) as usize;

        if msg_len + 4 > cap {
            return Err(RingError::Corrupted);
        }

        // Read message data
        let mut msg = vec![0u8; msg_len];
        unsafe {
            let msg_start = (data_offset + 4) % cap;
            if msg_start + msg_len <= cap {
                std::ptr::copy_nonoverlapping(data_ptr.add(msg_start), msg.as_mut_ptr(), msg_len);
            } else {
                for (i, b) in msg.iter_mut().enumerate() {
                    *b = *data_ptr.add((msg_start + i) % cap);
                }
            }
        }

        let consumed = 4 + msg_len;
        h.tail
            .store(tail.wrapping_add(consumed as u64), Ordering::Release);

        Ok(Some(msg))
    }

    /// Returns the number of messages currently in the buffer.
    pub fn len(&self) -> usize {
        let h = self.header();
        let head = h.head.load(Ordering::Relaxed);
        let tail = h.tail.load(Ordering::Relaxed);
        // Approximate: doesn't account for length prefixes
        head.wrapping_sub(tail) as usize
    }

    /// Returns true if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        let h = self.header();
        h.head.load(Ordering::Relaxed) == h.tail.load(Ordering::Relaxed)
    }
}

#[derive(Debug)]
pub enum RingError {
    MessageTooLarge,
    Corrupted,
}

impl std::fmt::Display for RingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RingError::MessageTooLarge => write!(f, "message too large for ring buffer"),
            RingError::Corrupted => write!(f, "ring buffer corrupted"),
        }
    }
}

impl std::error::Error for RingError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shm::ShmRegion;

    #[test]
    fn test_create_and_push_pop() {
        let region = ShmRegion::create("test_ring", HEADER_SIZE + 4096).unwrap();
        let ring = ShmRingBuffer::create(region.as_mut_ptr(), region.len(), 4096);

        // Empty initially
        assert!(ring.is_empty());
        assert!(ring.pop().unwrap().is_none());

        // Push a message
        let msg = b"hello world";
        assert!(ring.push(msg).unwrap());
        assert!(!ring.is_empty());

        // Pop it
        let popped = ring.pop().unwrap().unwrap();
        assert_eq!(popped, msg);

        // Now empty again
        assert!(ring.is_empty());
    }

    #[test]
    fn test_multiple_messages() {
        let region = ShmRegion::create("test_ring_multi", HEADER_SIZE + 4096).unwrap();
        let ring = ShmRingBuffer::create(region.as_mut_ptr(), region.len(), 4096);

        for i in 0..10 {
            let msg = format!("message {}", i);
            ring.push(msg.as_bytes()).unwrap();
        }

        for i in 0..10 {
            let popped = ring.pop().unwrap().unwrap();
            assert_eq!(popped, format!("message {}", i).as_bytes());
        }

        assert!(ring.pop().unwrap().is_none());
    }

    #[test]
    fn test_wrap_around() {
        let region = ShmRegion::create("test_ring_wrap", HEADER_SIZE + 64).unwrap();
        let ring = ShmRingBuffer::create(region.as_mut_ptr(), region.len(), 64);

        // Fill and drain multiple times to test wrap-around
        for _ in 0..20 {
            ring.push(b"short").unwrap();
            let popped = ring.pop().unwrap().unwrap();
            assert_eq!(popped, b"short");
        }
    }

    #[test]
    fn test_full_buffer() {
        let region = ShmRegion::create("test_ring_full", HEADER_SIZE + 32).unwrap();
        let ring = ShmRingBuffer::create(region.as_mut_ptr(), region.len(), 32);

        // Fill with messages until full
        let mut count = 0;
        while ring.push(b"msg").unwrap() {
            count += 1;
        }
        assert!(count > 0);

        // Should be full now
        assert!(!ring.push(b"msg").unwrap());

        // Drain all
        for _ in 0..count {
            ring.pop().unwrap().unwrap();
        }
        assert!(ring.is_empty());
    }
}
