//! soullink-shm — Zero-copy IPC via shared memory.
//!
//! Provides:
//! - `ShmRegion`: Anonymous shared memory (memfd_create + mmap)
//! - `ShmRingBuffer`: Lock-free SPSC ring buffer
//! - `ShmBus`: Cross-process broadcast bus
//! - `send_fd` / `recv_fd`: UDS fd-passing for mmap sharing

pub mod bus;
pub mod ring_buffer;
pub mod shm;
