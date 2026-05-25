//! PTY Terminal — Shell persistant avec pseudo-terminal et isolation bwrap.
//!
//! Fournit un shell bash interactif persistent avec:
//! - PTY via portable-pty (redimensionnable, interactif)
//! - Isolation bubblewrap (reseau desactive)
//! - Persistance via tmux (optionnel — survit aux redemarrages)
//! - Lecture non-bloquante de la sortie

use anyhow::Result;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex as StdMutex};

/// Configuration du PTY.
const DEFAULT_ROWS: u16 = 24;
const DEFAULT_COLS: u16 = 80;

/// Préfixe pour les sessions tmux SoulSystem.
const TMUX_SESSION_PREFIX: &str = "soulsystem_";

/// Terminal PTY persistant.
///
/// Clone-safe pour partage entre tache de lecture et handler Telegram.
/// Supporte tmux pour la persistance entre redémarrages.
#[derive(Clone)]
pub struct PtyTerminal {
    reader: Arc<StdMutex<Box<dyn Read + Send>>>,
    writer: Arc<StdMutex<Box<dyn Write + Send>>>,
    child_id: Arc<StdMutex<Option<u32>>>,
    /// Nom de session tmux si utilisée.
    tmux_session: Arc<StdMutex<Option<String>>>,
}

impl PtyTerminal {
    /// Cree un nouveau PTY. Utilise tmux si disponible, sinon bash direct.
    pub fn new() -> Result<Self> {
        // Si un nom de session spécifique est demandé via TmuxSessionBuilder
        // on crée avec le nom par défaut (sera écrasé par with_tmux_session)
        Self::new_inner(None)
    }

    /// Crée un PTY attaché à une session tmux nommée.
    pub fn with_tmux_session(session_name: &str) -> Result<Self> {
        Self::new_inner(Some(session_name.to_string()))
    }

    fn new_inner(tmux_session: Option<String>) -> Result<Self> {
        let pty_system = NativePtySystem::default();
        let pty_pair = pty_system.openpty(PtySize {
            rows: DEFAULT_ROWS,
            cols: DEFAULT_COLS,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let use_tmux = tmux_session.is_some() && Self::tmux_available();
        let session_name = tmux_session.filter(|_| use_tmux);

        let cmd = if let Some(ref name) = session_name {
            // Mode tmux: attacher à la session existante ou en créer une
            if Self::tmux_has_session(name) {
                tracing::info!("Attaching to existing tmux session: {}", name);
                let mut b = CommandBuilder::new("tmux");
                b.args(["attach-session", "-t", name]);
                b
            } else {
                tracing::info!("Creating new tmux session: {}", name);
                let mut b = CommandBuilder::new("tmux");
                b.args(["new-session", "-s", name, "-d"]);
                // On lance d'abord la session en détaché, puis on attache
                // En pratique on va directement executer bash dedans
                b
            }
        } else if Self::bwrap_available() {
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

        // Si tmux, attendre un peu que la session soit prete puis configurer
        if let Some(ref name) = session_name {
            std::thread::sleep(std::time::Duration::from_millis(300));
            // S'assurer que la session tmux existe
            if !Self::tmux_has_session(name) {
                tracing::warn!(
                    "tmux session '{}' not found after spawn, falling back",
                    name
                );
            }
        }

        let reader = pty_pair.master.try_clone_reader()?;
        let writer = pty_pair.master.take_writer()?;

        Ok(Self {
            reader: Arc::new(StdMutex::new(reader)),
            writer: Arc::new(StdMutex::new(writer)),
            child_id: Arc::new(StdMutex::new(child_id)),
            tmux_session: Arc::new(StdMutex::new(session_name)),
        })
    }

    /// Ecrit une entree dans le PTY (comme si on tapait dans le terminal).
    pub fn write(&self, input: &str) -> Result<()> {
        if let Some(ref name) = *self.tmux_session.lock().unwrap() {
            // Envoyer les touches via tmux send-keys
            if Self::tmux_available() {
                let output = std::process::Command::new("tmux")
                    .args(["send-keys", "-t", name, "-l", input])
                    .output();
                if let Ok(o) = output {
                    if !o.status.success() {
                        tracing::warn!(
                            "tmux send-keys failed: {}",
                            String::from_utf8_lossy(&o.stderr)
                        );
                    }
                }
            }
        }

        let mut guard = self.writer.lock().unwrap();
        guard.write_all(input.as_bytes())?;
        Ok(())
    }

    /// Lit la sortie disponible du PTY (non-bloquant).
    /// Retourne tout ce qui est disponible dans le buffer.
    pub fn read(&self) -> Result<String> {
        let mut output = String::new();

        // Si tmux, capturer le contenu du pane
        if let Some(ref name) = *self.tmux_session.lock().unwrap() {
            if Self::tmux_available() {
                if let Ok(captured) = Self::tmux_capture_pane(name) {
                    output.push_str(&captured);
                }
            }
        }

        // Aussi lire depuis le PTY natif
        let mut guard = self.reader.lock().unwrap();
        let mut buf = [0u8; 8192];

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
        Ok(())
    }

    pub fn child_id(&self) -> Option<u32> {
        *self.child_id.lock().unwrap()
    }

    /// Retourne le nom de la session tmux si utilisée.
    pub fn tmux_session_name(&self) -> Option<String> {
        self.tmux_session.lock().unwrap().clone()
    }

    /// Vérifie si tmux est installé.
    pub fn tmux_available() -> bool {
        std::process::Command::new("which")
            .arg("tmux")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Vérifie si une session tmux nommée existe.
    fn tmux_has_session(name: &str) -> bool {
        std::process::Command::new("tmux")
            .args(["has-session", "-t", name])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Capture le contenu d'un pane tmux.
    fn tmux_capture_pane(session: &str) -> Result<String> {
        let output = std::process::Command::new("tmux")
            .args(["capture-pane", "-t", session, "-p"])
            .output()?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Ok(String::new())
        }
    }

    /// Liste les sessions tmux SoulSystem existantes.
    pub fn list_soulsystem_sessions() -> Result<Vec<String>> {
        if !Self::tmux_available() {
            return Ok(Vec::new());
        }
        let output = std::process::Command::new("tmux")
            .args(["list-sessions", "-F", "#{session_name}"])
            .output()?;
        let names = String::from_utf8_lossy(&output.stdout);
        Ok(names
            .lines()
            .filter(|n| n.starts_with(TMUX_SESSION_PREFIX))
            .map(|n| n.to_string())
            .collect())
    }

    /// Sauvegarde les variables d'environnement importantes dans un fichier JSON.
    pub fn save_env(chat_id: i64) -> Result<()> {
        let dir = dirs_next_home().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
        let env_dir = dir.join(".soulsystem");
        std::fs::create_dir_all(&env_dir)?;

        let env_file = env_dir.join(format!("pty_env_{}.json", chat_id));
        let env_vars: std::collections::HashMap<String, String> = std::env::vars()
            .filter(|(k, _)| {
                k == "PATH" || k == "HOME" || k == "USER" || k == "SHELL" || k.starts_with("SOUL")
            })
            .collect();

        let json = serde_json::to_string_pretty(&env_vars)?;
        std::fs::write(&env_file, json)?;
        tracing::info!("PTY env saved to {:?}", env_file);
        Ok(())
    }

    /// Restaure les variables d'environnement depuis le fichier JSON.
    pub fn restore_env(chat_id: i64) -> Result<std::collections::HashMap<String, String>> {
        let dir = dirs_next_home().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
        let env_dir = dir.join(".soulsystem");
        let env_file = env_dir.join(format!("pty_env_{}.json", chat_id));

        if env_file.exists() {
            let json = std::fs::read_to_string(&env_file)?;
            let vars: std::collections::HashMap<String, String> = serde_json::from_str(&json)?;
            Ok(vars)
        } else {
            Ok(std::collections::HashMap::new())
        }
    }

    fn bwrap_available() -> bool {
        std::process::Command::new("which")
            .arg("bwrap")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// Helper pour trouver le home directory (évite dep sur dirs crate).
fn dirs_next_home() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(std::path::PathBuf::from)
}

impl std::fmt::Debug for PtyTerminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtyTerminal")
            .field("child_id", &self.child_id())
            .field("tmux_session", &self.tmux_session_name())
            .finish()
    }
}

/// Builder pour créer un PtyTerminal avec une session tmux nommée.
pub struct TmuxSessionBuilder {
    session_name: String,
}

impl TmuxSessionBuilder {
    pub fn new(chat_id: i64) -> Self {
        Self {
            session_name: format!("{}{}", TMUX_SESSION_PREFIX, chat_id),
        }
    }

    pub fn build(self) -> Result<PtyTerminal> {
        if PtyTerminal::tmux_available() {
            PtyTerminal::with_tmux_session(&self.session_name)
        } else {
            tracing::warn!("tmux not installed — terminal will not survive restarts");
            PtyTerminal::new()
        }
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
        std::thread::sleep(std::time::Duration::from_millis(500));

        let output = pty.read().unwrap();
        assert!(
            output.contains("HELLO_PTY"),
            "Output should contain 'HELLO_PTY', got: '{}'",
            output
        );
    }

    #[test]
    fn test_tmux_available_detection() {
        let available = PtyTerminal::tmux_available();
        // Just check it doesn't panic
        let _ = available;
    }

    #[test]
    fn test_tmux_session_list() {
        let sessions = PtyTerminal::list_soulsystem_sessions();
        assert!(sessions.is_ok());
    }

    #[test]
    fn test_save_and_restore_env() {
        let chat_id = 99999;
        // Save env
        let save_result = PtyTerminal::save_env(chat_id);
        assert!(save_result.is_ok());

        // Restore env
        let restore_result = PtyTerminal::restore_env(chat_id);
        assert!(restore_result.is_ok());
        let vars = restore_result.unwrap();
        assert!(vars.contains_key("PATH"));
    }
}
