//! PID file lock — prevents multiple gateway instances on the same port.
//!
//! On startup:
//!   1. Check if the target port is in use by attempting to connect.
//!   2. Write a PID file to /var/run/ (or a configurable path).
//!   3. On graceful shutdown, remove the PID file.
//!
//! The lock is advisory — it will not prevent a rogue process from
//! binding the same port, but it catches the common case of starting
//! a second instance with the same config.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Duration;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LockError {
    #[error("port {0} is already in use — cannot bind")]
    PortInUse(u16),

    #[error("gateway already running (PID {pid} from {path})")]
    AlreadyRunning { pid: u32, path: String },

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Gateway lock guard. Dropping or releasing cleans up the PID file.
#[derive(Debug)]
pub struct GatewayLock {
    pid_file: PathBuf,
    #[allow(dead_code)]
    pid: u32,
    released: bool,
}

impl GatewayLock {
    /// Try to acquire the lock. Checks port liveness and existing PID file.
    ///
    /// If the PID file exists and the process is alive, returns
    /// `Err(LockError::AlreadyRunning)`. If the port responds on TCP,
    /// returns `Err(LockError::PortInUse)`. Otherwise writes the PID file
    /// and returns the lock guard.
    pub fn acquire(port: u16, pid_dir: Option<PathBuf>) -> Result<Self, LockError> {
        let dir = pid_dir.unwrap_or_else(|| PathBuf::from("/var/run"));
        std::fs::create_dir_all(&dir)?;
        let pid_file = dir.join(format!("soullink-gateway-{port}.pid"));

        let current_pid = std::process::id();

        // 1. Check existing PID file
        if pid_file.exists() {
            let mut contents = String::new();
            let mut f = std::fs::File::open(&pid_file)?;
            f.read_to_string(&mut contents)?;
            if let Ok(old_pid) = contents.trim().parse::<u32>() {
                // Check if process is alive
                #[cfg(unix)]
                {
                    let ret = unsafe { libc::kill(old_pid as i32, 0) };
                    if ret == 0 {
                        return Err(LockError::AlreadyRunning {
                            pid: old_pid,
                            path: pid_file.display().to_string(),
                        });
                    }
                }
                #[cfg(not(unix))]
                {
                    // On non-Unix, just try opening a TCP connection
                    if port_check(port) {
                        return Err(LockError::PortInUse(port));
                    }
                }
            }
        }

        // 2. Quick TCP check — if the port responds, something is already listening
        if port_check(port) {
            return Err(LockError::PortInUse(port));
        }

        // 3. Write PID file
        let mut f = std::fs::File::create(&pid_file)?;
        write!(f, "{current_pid}")?;
        f.sync_all()?;

        Ok(Self {
            pid_file,
            pid: current_pid,
            released: false,
        })
    }

    /// Explicitly release the lock (removes PID file).
    pub fn release(&mut self) {
        if !self.released {
            self.released = true;
            let _ = std::fs::remove_file(&self.pid_file);
        }
    }
}

impl Drop for GatewayLock {
    fn drop(&mut self) {
        self.release();
    }
}

/// Quick TCP reachability check on localhost.
fn port_check(port: u16) -> bool {
    use std::net::{TcpStream, ToSocketAddrs};
    let addr = format!("127.0.0.1:{port}");
    if let Ok(mut addrs) = addr.to_socket_addrs() {
        if let Some(addr) = addrs.next() {
            return TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok();
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_port_check_free_port() {
        // Port 1 is typically not listening
        assert!(!port_check(1));
    }

    #[test]
    fn test_acquire_and_release() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut lock = GatewayLock::acquire(19990, Some(dir.path().into())).expect("acquire lock");
        assert_eq!(lock.pid, std::process::id());
        assert!(!lock.released);

        // PID file should exist
        let pid_path = dir.path().join("soullink-gateway-19990.pid");
        assert!(pid_path.exists());

        lock.release();
        assert!(lock.released);
        assert!(!pid_path.exists());
    }

    #[test]
    fn test_acquire_twice_same_dir_fails() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _lock = GatewayLock::acquire(19991, Some(dir.path().into())).expect("first acquire");

        let result = GatewayLock::acquire(19991, Some(dir.path().into()));
        assert!(result.is_err());
        match result {
            Err(LockError::AlreadyRunning { .. }) => {} // expected
            _ => panic!("expected AlreadyRunning error, got: {result:?}"),
        }
    }

    #[test]
    fn test_drop_releases() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pid_path = dir.path().join("soullink-gateway-19992.pid");
        {
            let _lock = GatewayLock::acquire(19992, Some(dir.path().into())).expect("acquire");
            assert!(pid_path.exists());
        }
        // After drop, PID file should be gone
        assert!(!pid_path.exists());
    }
}
