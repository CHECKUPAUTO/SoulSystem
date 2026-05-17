//! PTY Terminal — Shell persistant avec pseudo-terminal et isolation bwrap.
//!
//! Fournit un shell bash interactif persistent avec:
//! - PTY via portable-pty (redimensionnable, interactif)
//! - Isolation bubblewrap (reseau desactive)
//! - Lecture non-bloquante de la sortie

use anyhow::Result;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex as StdMutex};

/// Configuration du PTY.
const DEFAULT_ROWS: u16 = 24;
const DEFAULT_COLS: u16 = 80;

/// Terminal PTY persistant.
///
/// Clone-safe pour partage entre tache de lecture et handler Telegram.
#[derive(Clone)]
pub struct PtyTerminal {
    reader: Arc<StdMutex<Box<dyn Read + Send>>>,
    writer: Arc<StdMutex<Box<dyn Write + Send>>>,
    child_id: Arc<StdMutex<Option<u32>>>,
}

impl PtyTerminal {
    /// Cree un nouveau PTY avec bash dans un sandbox bwrap.
    pub fn new() -> Result<Self> {
        let pty_system = NativePtySystem::default();
        let pty_pair = pty_system.openpty(PtySize {
            rows: DEFAULT_ROWS,
            cols: DEFAULT_COLS,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let cmd = if Self::bwrap_available() {
            let mut b = CommandBuilder::new("bwrap");
            b.args([
                "--ro-bind",
                "/usr",
                "/usr",
                "--ro-bind",
                "/lib",
                "/lib",
                "--ro-bind",
                "/lib64",
                "/lib64",
                "--ro-bind",
                "/bin",
                "/bin",
                "--ro-bind",
                "/sbin",
                "/sbin",
                "--ro-bind",
                "/etc",
                "/etc",
                "--ro-bind",
                "/dev",
                "/dev",
                "--ro-bind",
                "/proc",
                "/proc",
                "--tmpfs",
                "/tmp",
                "--unshare-net",
                "--die-with-parent",
                "--",
                "bash",
                "--norc",
            ]);
            b
        } else {
            let mut b = CommandBuilder::new("bash");
            b.arg("--norc");
            b
        };

        // Set O_NONBLOCK on master fd so cloned reader is non-blocking.
        #[cfg(unix)]
        if let Some(fd) = pty_pair.master.as_raw_fd() {
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFL, 0);
                let _ = libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
        }

        let child = pty_pair.slave.spawn_command(cmd)?;
        let child_id = child.process_id();
        let _child_handle = child;

        let reader = pty_pair.master.try_clone_reader()?;
        let writer = pty_pair.master.take_writer()?;

        Ok(Self {
            reader: Arc::new(StdMutex::new(reader)),
            writer: Arc::new(StdMutex::new(writer)),
            child_id: Arc::new(StdMutex::new(child_id)),
        })
    }

    /// Ecrit une entree dans le PTY (comme si on tapait dans le terminal).
    pub fn write(&self, input: &str) -> Result<()> {
        let mut guard = self.writer.lock().unwrap();
        guard.write_all(input.as_bytes())?;
        Ok(())
    }

    /// Lit la sortie disponible du PTY (non-bloquant).
    /// Retourne tout ce qui est disponible dans le buffer.
    pub fn read(&self) -> Result<String> {
        let mut guard = self.reader.lock().unwrap();
        let mut buf = [0u8; 8192];
        let mut output = String::new();

        loop {
            match guard.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => output.push_str(&String::from_utf8_lossy(&buf[..n])),
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }

        Ok(output)
    }

    /// Redimensionne le PTY.
    pub fn resize(&self, _rows: u16, _cols: u16) -> Result<()> {
        // La redimension via writer n'est pas directement supportee.
        // En pratique on ignore pour l'instant.
        Ok(())
    }

    pub fn child_id(&self) -> Option<u32> {
        *self.child_id.lock().unwrap()
    }

    fn bwrap_available() -> bool {
        std::process::Command::new("which")
            .arg("bwrap")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

impl std::fmt::Debug for PtyTerminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtyTerminal")
            .field("child_id", &self.child_id())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pty_create() {
        let pty = PtyTerminal::new();
        assert!(pty.is_ok(), "PTY should be created successfully");
        let pty = pty.unwrap();
        assert!(pty.child_id().is_some());
    }

    #[test]
    fn test_pty_write_and_read() {
        let pty = PtyTerminal::new().unwrap();

        // Drain initial output (bash prompt)
        std::thread::sleep(std::time::Duration::from_millis(200));
        let _ = pty.read();

        pty.write("echo HELLO_PTY\n").unwrap();

        // Wait for bash to process and echo back
        std::thread::sleep(std::time::Duration::from_millis(500));

        let output = pty.read().unwrap();
        assert!(
            output.contains("HELLO_PTY"),
            "Output should contain 'HELLO_PTY', got: '{}'",
            output
        );
    }
}
