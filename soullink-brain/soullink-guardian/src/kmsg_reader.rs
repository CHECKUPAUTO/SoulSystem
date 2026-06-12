//! KmsgReader — tails `/dev/kmsg` for real NVIDIA XID errors.
//!
//! # Why this actor
//!
//! The V13.5 p1a hotfix proved that `nvml.throttle_reasons()` is an
//! **informational bitmask**, not an error signal. The only canonical
//! source of NVIDIA XID errors is the kernel log, emitted by the `nvidia`
//! module as:
//!
//! ```text
//! NVRM: Xid (PCI:0000:03:00): 43, pid=<pid>, Ch 00000008
//! ```
//!
//! This actor opens `/dev/kmsg` read-only, advances to the end on startup
//! (so we don't replay historical XIDs from before boot), parses each new
//! line with a strict regex, and emits [`GuardianEvent::XidError`] with
//! the genuine NVIDIA error code.
//!
//! `EmergencyReflex::on_xid_event` then decides whether to reset based on
//! [`crate::emergency::FATAL_XID_CODES`].
//!
//! # Parsing
//!
//! The regex intentionally ignores the leading kmsg priority/sequence
//! prefix (`6,12345,...;`) and matches only the message body. Lines that
//! don't match are dropped silently (kmsg is very chatty).
//!
//! # Non-Linux builds
//!
//! On non-Linux platforms this module compiles to a no-op (the `run`
//! method immediately returns). Lets the guardian build on dev machines
//! and CI without NVIDIA/kernel log access.

use std::{sync::Arc, time::Duration};

use tokio::{
    sync::{broadcast, mpsc},
    time,
};
use tracing::{debug, info, warn};

use soullink_core::GuardianEvent;

/// Open `/dev/kmsg` here if available at startup.
#[cfg(target_os = "linux")]
pub const KMSG_PATH: &str = "/dev/kmsg";

// ─── Parser (re-exported from soullink-core for benchability) ───────────────

pub use soullink_core::kmsg_parse::parse_xid_line;

// ─── Actor ───────────────────────────────────────────────────────────────────

pub struct KmsgReader {
    tx: mpsc::Sender<GuardianEvent>,
    shutdown: broadcast::Receiver<()>,
    /// Monotonic counter of XIDs seen since startup. Propagated in
    /// `GuardianEvent::XidError { count }`.
    xid_count: Arc<std::sync::atomic::AtomicU64>,
}

impl KmsgReader {
    pub fn new(tx: mpsc::Sender<GuardianEvent>, shutdown: broadcast::Receiver<()>) -> Self {
        Self {
            tx,
            shutdown,
            xid_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    pub fn xid_count(&self) -> Arc<std::sync::atomic::AtomicU64> {
        self.xid_count.clone()
    }

    #[cfg(target_os = "linux")]
    pub async fn run(mut self) {
        use std::io::Read;
        use std::os::fd::AsRawFd;

        info!(path = KMSG_PATH, "KmsgReader starting");

        let mut f = match std::fs::OpenOptions::new().read(true).open(KMSG_PATH) {
            Ok(f) => f,
            Err(e) => {
                warn!(%e, "KmsgReader: cannot open /dev/kmsg — XID detection disabled");
                return;
            }
        };

        // Seek to end: skip historical XIDs from before this process started.
        unsafe {
            libc_seek_end(f.as_raw_fd());
        }
        // Non-blocking: read() returns EAGAIN when no new message is ready.
        unsafe {
            libc_set_nonblock(f.as_raw_fd());
        }

        // /dev/kmsg delivers one message per successful read(). Max kernel
        // message size is 8 KiB on Linux — 16 KiB buffer is safely above.
        let mut buf = vec![0u8; 16 * 1024];

        let mut ticker = time::interval(Duration::from_millis(100));
        ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    // Drain everything available this tick
                    loop {
                        match f.read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => {
                                // kmsg guarantees one full message per read.
                                // Convert with lossy UTF-8 — kernel messages
                                // are normally ASCII but be defensive.
                                let line = String::from_utf8_lossy(&buf[..n]);
                                if let Some(code) = parse_xid_line(&line) {
                                    let count = self
                                        .xid_count
                                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                                        + 1;
                                    info!(xid_code = code, total = count,
                                          "real XID detected via kmsg");
                                    let _ = self.tx.try_send(GuardianEvent::XidError {
                                        code, count,
                                    });
                                }
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(e) if e.raw_os_error() == Some(libc_EPIPE) => {
                                // Ring buffer overflow — we missed messages.
                                // Not fatal; log and continue (kmsg resumes).
                                warn!("kmsg ring buffer overflow — some messages dropped");
                                continue;
                            }
                            Err(e) => {
                                debug!(%e, "kmsg read error (non-fatal)");
                                break;
                            }
                        }
                    }
                }
                _ = self.shutdown.recv() => {
                    info!("KmsgReader: shutdown");
                    break;
                }
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub async fn run(self) {
        let mut shutdown = self.shutdown;
        info!("KmsgReader: non-Linux build — XID detection disabled (no-op)");
        let _ = shutdown.recv().await;
    }
}

// ─── minimal libc helpers (no libc crate dependency) ─────────────────────────

#[cfg(target_os = "linux")]
extern "C" {
    fn lseek(fd: i32, offset: i64, whence: i32) -> i64;
    fn fcntl(fd: i32, cmd: i32, arg: i32) -> i32;
}

#[cfg(target_os = "linux")]
const SEEK_END: i32 = 2;
#[cfg(target_os = "linux")]
const F_GETFL: i32 = 3;
#[cfg(target_os = "linux")]
const F_SETFL: i32 = 4;
#[cfg(target_os = "linux")]
const O_NONBLOCK: i32 = 0o4000;

#[cfg(target_os = "linux")]
#[allow(non_upper_case_globals)]
const libc_EPIPE: i32 = 32;

#[cfg(target_os = "linux")]
#[allow(non_snake_case)]
unsafe fn libc_seek_end(fd: i32) {
    unsafe {
        lseek(fd, 0, SEEK_END);
    }
}

#[cfg(target_os = "linux")]
#[allow(non_snake_case)]
unsafe fn libc_set_nonblock(fd: i32) {
    unsafe {
        let flags = fcntl(fd, F_GETFL, 0);
        if flags >= 0 {
            fcntl(fd, F_SETFL, flags | O_NONBLOCK);
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

// Parser tests live in soullink-core::kmsg_parse — we only re-export here,
// so one smoke-test is enough to guarantee the path works.
#[cfg(test)]
mod tests {
    use super::parse_xid_line;

    #[test]
    fn reexport_is_wired() {
        assert_eq!(parse_xid_line("NVRM: Xid (PCI:0000:03:00): 43"), Some(43));
    }
}
