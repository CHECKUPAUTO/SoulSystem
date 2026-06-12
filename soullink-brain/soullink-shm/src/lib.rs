#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    unused_mut,
    unused_unsafe,
    unreachable_pub,
    non_camel_case_types,
    non_snake_case,
    unused_comparisons
)]
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
