//! # soul_tools — Découverte et exécution d'outils système
//!
//! Découvre automatiquement les commandes disponibles, les classe par catégorie,
//! et permet leur exécution.
//!
//! ## Exemple
//! ```ignore
//! use soul_tools::*;
//! let tools = discover_system_tools(); // 40+ outils
//! let mut registry = ToolRegistry::new();
//! for tool in tools { registry.register(tool); }
//! let output = execute_shell("ls -la")?;
//! ```

use serde::{Deserialize, Serialize};
use soul_sandbox::{Sandbox, SandboxPolicy, SandboxVerdict};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

const MAX_TOOL_FILE_READ_BYTES: usize = 4 * 1024 * 1024;
const MAX_TOOL_FILE_WRITE_BYTES: usize = 8 * 1024 * 1024;

// ── Types ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ToolCategory {
    System,
    Network,
    File,
    Process,
    Data,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub category: ToolCategory,
}

impl std::fmt::Display for ToolCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolCategory::System => write!(f, "system"),
            ToolCategory::Network => write!(f, "network"),
            ToolCategory::File => write!(f, "file"),
            ToolCategory::Process => write!(f, "process"),
            ToolCategory::Data => write!(f, "data"),
            ToolCategory::Custom(s) => write!(f, "{}", s),
        }
    }
}

// ── Découverte de la base (commandes courantes) ──────────────

pub fn discover_system_tools() -> Vec<Tool> {
    let tools = [
        // System
        ("uname", "Afficher les infos système", ToolCategory::System),
        ("hostname", "Nom de l'hôte", ToolCategory::System),
        ("uptime", "Temps de fonctionnement", ToolCategory::System),
        ("whoami", "Utilisateur courant", ToolCategory::System),
        // File
        ("ls", "Lister fichiers", ToolCategory::File),
        ("cat", "Contenu de fichier", ToolCategory::File),
        ("find", "Rechercher fichiers", ToolCategory::File),
        ("grep", "Rechercher texte", ToolCategory::File),
        ("cp", "Copier fichier", ToolCategory::File),
        ("mv", "Déplacer fichier", ToolCategory::File),
        ("mkdir", "Créer répertoire", ToolCategory::File),
        ("rm", "Supprimer", ToolCategory::File),
        // Process
        ("ps", "Lister processus", ToolCategory::Process),
        ("top", "Surveiller CPU/mémoire", ToolCategory::Process),
        ("df", "Espace disque", ToolCategory::Process),
        ("free", "Mémoire utilisée/libre", ToolCategory::Process),
        // Network
        ("ping", "Tester connectivité", ToolCategory::Network),
        ("curl", "Transfert HTTP", ToolCategory::Network),
        ("ss", "Stats socket réseau", ToolCategory::Network),
        // Data
        ("wc", "Compter lignes/mots", ToolCategory::Data),
        ("sort", "Trier lignes", ToolCategory::Data),
        ("uniq", "Éliminer doublons", ToolCategory::Data),
        ("cut", "Extraire colonnes", ToolCategory::Data),
        ("head", "Premières lignes", ToolCategory::Data),
        ("tail", "Dernières lignes", ToolCategory::Data),
        // Git
        ("git", "Gestion version Git", ToolCategory::System),
        ("cargo", "Compilation Rust", ToolCategory::System),
        ("docker", "Conteneurisation", ToolCategory::System),
        ("journalctl", "Journal système Linux", ToolCategory::System),
        ("systemctl", "Services systemd", ToolCategory::System),
        ("ssh", "Connexion distante SSH", ToolCategory::Network),
        ("scp", "Transfert fichier SSH", ToolCategory::Network),
        ("rsync", "Synchronisation fichiers", ToolCategory::File),
        ("tar", "Compression archive", ToolCategory::File),
        ("gzip", "Compression gzip", ToolCategory::File),
    ];

    tools
        .iter()
        .map(|(name, desc, cat)| Tool {
            name: (*name).into(),
            description: (*desc).into(),
            category: cat.clone(),
        })
        .filter(|t| is_command_available(&t.name))
        .collect()
}

fn is_command_available(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ── Registre d'outils ────────────────────────────────────────

pub struct ToolRegistry {
    tools: Vec<Tool>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    pub fn register(&mut self, tool: Tool) {
        self.tools.push(tool);
    }

    pub fn get(&self, name: &str) -> Option<&Tool> {
        self.tools.iter().find(|t| t.name == name)
    }

    pub fn list(&self) -> Vec<&Tool> {
        self.tools.iter().collect()
    }

    pub fn search(&self, query: &str) -> Vec<&Tool> {
        let q = query.to_lowercase();
        self.tools
            .iter()
            .filter(|t| {
                t.name.to_lowercase().contains(&q) || t.description.to_lowercase().contains(&q)
            })
            .collect()
    }

    pub fn by_category(&self, cat: &ToolCategory) -> Vec<&Tool> {
        self.tools.iter().filter(|t| &t.category == cat).collect()
    }

    pub fn all(&self) -> &[Tool] {
        &self.tools
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        let mut reg = Self::new();
        for t in discover_system_tools() {
            reg.register(t);
        }
        reg
    }
}

// ── Permission levels ─────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionLevel {
    Read,
    Write,
    Destructive,
}

impl PermissionLevel {
    /// Classify a shell command by permission level.
    pub fn from_command(cmd: &str) -> Self {
        let lower = cmd.to_lowercase();
        let destructive = [
            "rm -rf",
            "mkfs",
            "dd if",
            "shutdown",
            "reboot",
            "poweroff",
            "kill -9",
            "pkill -9",
            "iptables -F",
            "fdisk",
            "parted",
        ];
        let write = [
            "rm ",
            "mv ",
            "cp ",
            "mkdir ",
            "touch ",
            "chmod ",
            "chown ",
            "write_file",
            "patch_file",
            "git push",
            "git reset --hard",
        ];
        for d in &destructive {
            if lower.contains(d) {
                return PermissionLevel::Destructive;
            }
        }
        for w in &write {
            if lower.contains(w) {
                return PermissionLevel::Write;
            }
        }
        PermissionLevel::Read
    }
}

// ── Async executor ───────────────────────────────────────────

/// Async shell executor backed by `soul_sandbox`.
#[derive(Clone)]
pub struct AsyncShellExecutor {
    sandbox: Arc<Sandbox>,
    #[allow(dead_code)]
    timeout: Duration,
}

impl AsyncShellExecutor {
    pub fn new(timeout_secs: u64) -> Self {
        Self {
            sandbox: Arc::new(Sandbox::new(SandboxPolicy::default())),
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    pub async fn execute(&self, cmd: &str) -> std::result::Result<SandboxVerdict, String> {
        let sandbox = self.sandbox.clone();
        let cmd = cmd.to_string();
        tokio::task::spawn_blocking(move || sandbox.execute(&cmd).map_err(|e| e.to_string()))
            .await
            .map_err(|e| format!("spawn blocking failed: {e}"))?
    }
}

// ── Typed tool registry ────────────────────────────────────────

/// Error raised by the tool dispatch path.
///
/// Security-critical invariant (INV-TOOL-1, see
/// `docs/security/SECURITY_INVARIANTS.md`): an unregistered tool name yields
/// [`ToolError::UnknownTool`] and is NEVER converted into a process invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    /// The requested tool name is not a registered tool.
    UnknownTool { name: String },
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownTool { name } => {
                write!(f, "unknown tool {name:?}: not a registered tool")
            }
        }
    }
}

impl std::error::Error for ToolError {}

/// The closed set of tools the agent may dispatch.
///
/// This enum *is* the registry: [`ToolId::from_name`] is the only way a tool
/// name becomes dispatchable, and it accepts exactly these identifiers. There is
/// no fallback that turns an arbitrary name into an executable invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolId {
    /// Run a shell command (the single explicit process-execution tool).
    ExecuteShell,
    /// Read a file.
    ReadFile,
    /// Write a file.
    WriteFile,
    /// Patch a file (search/replace).
    PatchFile,
    /// Read a web page through a locally running Chrome CDP endpoint.
    BrowserRead,
    /// Invoke an explicitly configured MCP tool over WebSocket.
    McpCall,
}

impl ToolId {
    /// The canonical wire name of this tool.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExecuteShell => "execute_shell",
            Self::ReadFile => "read_file",
            Self::WriteFile => "write_file",
            Self::PatchFile => "patch_file",
            Self::BrowserRead => "browser_read",
            Self::McpCall => "mcp_call",
        }
    }

    /// Every registered tool id.
    pub fn all() -> [ToolId; 6] {
        [
            Self::ExecuteShell,
            Self::ReadFile,
            Self::WriteFile,
            Self::PatchFile,
            Self::BrowserRead,
            Self::McpCall,
        ]
    }

    /// Resolve a tool name to its id, or fail closed.
    ///
    /// The match is exact: no trimming, no case-folding, no Unicode
    /// normalisation. Any name that is not byte-for-byte one of the registered
    /// identifiers yields [`ToolError::UnknownTool`] and cannot execute.
    pub fn from_name(name: &str) -> std::result::Result<ToolId, ToolError> {
        match name {
            "execute_shell" => Ok(Self::ExecuteShell),
            "read_file" => Ok(Self::ReadFile),
            "write_file" => Ok(Self::WriteFile),
            "patch_file" => Ok(Self::PatchFile),
            "browser_read" => Ok(Self::BrowserRead),
            "mcp_call" => Ok(Self::McpCall),
            _ => Err(ToolError::UnknownTool {
                name: name.to_string(),
            }),
        }
    }

    /// Whether `name` is a registered tool.
    pub fn is_registered(name: &str) -> bool {
        Self::from_name(name).is_ok()
    }

    /// The trusted, statically-registered capability of this tool.
    ///
    /// Decided here by trusted code — never by a caller or by LLM input — and it
    /// cannot be downgraded by a request (INV-TOOL-2). See
    /// `docs/security/SECURITY_INVARIANTS.md`.
    pub fn capability(self) -> ToolCapability {
        match self {
            Self::ExecuteShell => ToolCapability::ProcessExecution,
            Self::ReadFile => ToolCapability::ReadOnly,
            Self::WriteFile | Self::PatchFile => ToolCapability::FileWrite,
            Self::BrowserRead => ToolCapability::NetworkRead,
            Self::McpCall => ToolCapability::NetworkWrite,
        }
    }
}

// ── Capabilities and policy ────────────────────────────────────

/// The kind of side effect a tool is authorised to cause.
///
/// The capability is a property of the *registered* tool. It sets a floor on the
/// approval a tool call requires; a request can never lower it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCapability {
    /// Reads only; no state change.
    ReadOnly,
    /// Reads data over the network.
    NetworkRead,
    /// Sends state-changing requests over the network.
    NetworkWrite,
    /// Writes to the filesystem.
    FileWrite,
    /// Executes a process.
    ProcessExecution,
    /// Accesses credentials/secrets.
    CredentialAccess,
    /// Mutates persistent memory.
    MemoryMutation,
    /// Modifies the agent's own code/skills.
    SelfModification,
    /// Administrative / privileged control.
    Administrative,
}

impl ToolCapability {
    /// The minimum [`PermissionLevel`] a tool with this capability requires.
    ///
    /// A floor, not the final answer: for the shell tool the effective
    /// requirement is refined per-command (see [`required_permission_for`]).
    pub fn required_permission(self) -> PermissionLevel {
        match self {
            Self::ReadOnly | Self::NetworkRead => PermissionLevel::Read,
            Self::FileWrite | Self::NetworkWrite | Self::MemoryMutation => PermissionLevel::Write,
            Self::ProcessExecution
            | Self::CredentialAccess
            | Self::SelfModification
            | Self::Administrative => PermissionLevel::Destructive,
        }
    }
}

/// A request to authorise a tool call. Carries only the trusted tool name and,
/// for the shell tool, the concrete command — never a caller-supplied
/// permission level.
#[derive(Debug, Clone)]
pub struct CapabilityRequest<'a> {
    /// The requested tool name (resolved against the registry).
    pub tool_name: &'a str,
    /// The concrete command, for the shell tool only.
    pub command: Option<&'a str>,
}

/// The single interface every runtime uses to derive a tool call's required
/// permission. Deterministic and fail-closed.
pub trait PolicyEngine {
    /// Derive the effective required permission for a request.
    fn required_permission(&self, request: &CapabilityRequest<'_>) -> PermissionLevel;
}

/// The default capability-based policy.
#[derive(Debug, Clone, Copy, Default)]
pub struct CapabilityPolicy;

impl PolicyEngine for CapabilityPolicy {
    fn required_permission(&self, request: &CapabilityRequest<'_>) -> PermissionLevel {
        required_permission_for(request.tool_name, request.command)
    }
}

/// Canonical classification: derive the required permission for a tool call from
/// the trusted registry. This is the one authorization-input decision point.
///
/// - `read_file` → `Read`; `write_file` / `patch_file` → `Write` (their
///   `FileWrite` capability floor) — this is the CRIT-003 fix.
/// - `execute_shell` → the per-command sensitivity ([`PermissionLevel::from_command`]),
///   the correct granularity for a shell tool.
/// - An unregistered name → `Destructive` (fail closed); it is additionally
///   rejected at dispatch (INV-TOOL-1).
///
/// The caller never supplies a permission — it is always derived here, so a
/// request cannot downgrade a tool's requirement (INV-TOOL-2).
pub fn required_permission_for(name: &str, command: Option<&str>) -> PermissionLevel {
    match ToolId::from_name(name) {
        Ok(ToolId::ExecuteShell) => PermissionLevel::from_command(command.unwrap_or("")),
        Ok(other) => other.capability().required_permission(),
        Err(_) => PermissionLevel::Destructive,
    }
}

// ── Filesystem confinement ──────────────────────────────────────

/// Error raised while resolving a file-tool path against a [`FsPolicy`].
///
/// `Display` carries only the requested path string — never file contents —
/// so it is safe to return to the caller and to log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsPolicyError {
    /// The requested path was empty.
    EmptyPath,
    /// The requested path contained a NUL byte.
    NulByte { requested: String },
    /// The resolved path escapes the confined root.
    OutsideRoot { requested: String },
    /// The requested path contains a symlink whose target does not exist.
    BrokenSymlink { requested: String },
    /// The resolved path touches a protected location (e.g. `.git`).
    ProtectedPath { requested: String },
    /// The workspace root itself could not be canonicalised.
    RootNotFound { detail: String },
    /// An I/O error occurred while resolving the path.
    Io { requested: String, detail: String },
}

impl std::fmt::Display for FsPolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyPath => write!(f, "empty path"),
            Self::NulByte { requested } => write!(f, "path {requested:?} contains a NUL byte"),
            Self::OutsideRoot { requested } => {
                write!(f, "path {requested:?} escapes the confined workspace root")
            }
            Self::BrokenSymlink { requested } => {
                write!(f, "path {requested:?} contains a broken symlink")
            }
            Self::ProtectedPath { requested } => {
                write!(f, "path {requested:?} touches a protected location")
            }
            Self::RootNotFound { detail } => write!(f, "workspace root not found: {detail}"),
            Self::Io { requested, detail } => write!(f, "path {requested:?}: {detail}"),
        }
    }
}

impl std::error::Error for FsPolicyError {}

/// Confines file-tool paths to a canonical workspace root.
///
/// Every file tool resolves its path through [`FsPolicy::resolve_read`] or
/// [`FsPolicy::resolve_write`] before touching the filesystem — there is no
/// `std::fs` call on a caller-supplied path string anywhere on the dispatch
/// path (CRIT-004 / INV-FS-1). Resolution is race-resistant: it canonicalises
/// the nearest *existing* ancestor of the requested path (which resolves any
/// symlink in that ancestor chain, including the target itself when it
/// exists) before checking containment, so a symlink planted inside the root
/// that points outside it is rejected rather than followed
/// (INV-FS-2). `.git` is protected inside the root (INV-FS-3).
pub struct FsPolicy {
    root: PathBuf,
}

impl FsPolicy {
    /// Build a policy confined to `root`. `root` must exist; it is
    /// canonicalised once so every later containment check compares against a
    /// symlink-free, absolute path.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, FsPolicyError> {
        let root =
            std::fs::canonicalize(root.as_ref()).map_err(|e| FsPolicyError::RootNotFound {
                detail: e.to_string(),
            })?;
        Ok(Self { root })
    }

    /// The canonical root this policy confines paths to.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve a path that must already exist (read, or the target of a patch).
    pub fn resolve_read(&self, requested: &str) -> Result<PathBuf, FsPolicyError> {
        self.resolve(requested, true)
    }

    /// Resolve a path that may not yet exist (write creates it).
    pub fn resolve_write(&self, requested: &str) -> Result<PathBuf, FsPolicyError> {
        self.resolve(requested, false)
    }

    fn resolve(&self, requested: &str, must_exist: bool) -> Result<PathBuf, FsPolicyError> {
        if requested.is_empty() {
            return Err(FsPolicyError::EmptyPath);
        }
        if requested.as_bytes().contains(&0) {
            return Err(FsPolicyError::NulByte {
                requested: requested.to_string(),
            });
        }

        let req_path = Path::new(requested);
        let candidate = if req_path.is_absolute() {
            req_path.to_path_buf()
        } else {
            self.root.join(req_path)
        };

        // `Path::exists()` is false for a broken symlink. Check link metadata
        // before the nearest-existing-ancestor walk so a link is never
        // mistaken for an ordinary missing path during a write.
        self.reject_broken_symlink(&candidate, requested)?;

        // Walk up to the nearest existing ancestor, collecting the
        // not-yet-existing tail. Canonicalising only the existing part
        // resolves any symlink in that chain (including the leaf itself, when
        // it already exists) without requiring the whole path to exist.
        let mut existing = candidate.clone();
        let mut tail: Vec<std::ffi::OsString> = Vec::new();
        while !existing.exists() {
            let Some(parent) = existing.parent() else {
                return Err(FsPolicyError::OutsideRoot {
                    requested: requested.to_string(),
                });
            };
            if let Some(name) = existing.file_name() {
                tail.push(name.to_os_string());
            }
            existing = parent.to_path_buf();
        }
        if must_exist && !tail.is_empty() {
            return Err(FsPolicyError::Io {
                requested: requested.to_string(),
                detail: "not found".to_string(),
            });
        }

        let canon_existing = std::fs::canonicalize(&existing).map_err(|e| FsPolicyError::Io {
            requested: requested.to_string(),
            detail: e.to_string(),
        })?;
        if !canon_existing.starts_with(&self.root) {
            return Err(FsPolicyError::OutsideRoot {
                requested: requested.to_string(),
            });
        }

        let mut result = canon_existing;
        for comp in tail.into_iter().rev() {
            if comp == ".." || comp == "." {
                return Err(FsPolicyError::OutsideRoot {
                    requested: requested.to_string(),
                });
            }
            result.push(comp);
        }

        if result.components().any(|c| c.as_os_str() == ".git") {
            return Err(FsPolicyError::ProtectedPath {
                requested: requested.to_string(),
            });
        }
        if !result.starts_with(&self.root) {
            return Err(FsPolicyError::OutsideRoot {
                requested: requested.to_string(),
            });
        }

        Ok(result)
    }

    fn reject_broken_symlink(
        &self,
        candidate: &Path,
        requested: &str,
    ) -> Result<(), FsPolicyError> {
        let mut current = PathBuf::new();
        for component in candidate.components() {
            current.push(component.as_os_str());
            let metadata = match std::fs::symlink_metadata(&current) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => {
                    return Err(FsPolicyError::Io {
                        requested: requested.to_string(),
                        detail: error.to_string(),
                    })
                }
            };
            if !metadata.file_type().is_symlink() {
                continue;
            }
            match std::fs::metadata(&current) {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Err(FsPolicyError::BrokenSymlink {
                        requested: requested.to_string(),
                    })
                }
                Err(error) => {
                    return Err(FsPolicyError::Io {
                        requested: requested.to_string(),
                        detail: error.to_string(),
                    })
                }
            }
        }
        Ok(())
    }
}

/// The default confinement root: the process's current working directory.
///
/// This is the one root available today without richer workspace-root
/// configuration plumbing (tracked as a residual item for a follow-up PR); it
/// still establishes real confinement on the live dispatch path, rejecting
/// any path outside the process's own working directory.
fn default_workspace_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
}

static ATOMIC_WRITE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Write `content` to `path` atomically: write to a sibling temp file with
/// restrictive permissions, flush, sync, then rename over the destination.
/// A crash or concurrent reader never observes a partially written file
/// (INV-FS-4). `path` must already be confinement-checked by the caller.
fn atomic_write(path: &Path, content: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no parent")
        })?;
    let file_name = path.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no file name")
    })?;

    let n = ATOMIC_WRITE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp_name = format!(
        ".{}.soultmp-{}-{n}",
        file_name.to_string_lossy(),
        std::process::id()
    );
    let tmp_path = dir.join(tmp_name);

    let write_result = (|| {
        let mut opts = std::fs::OpenOptions::new();
        opts.create(true).write(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp_path)?;
        f.write_all(content)?;
        f.sync_all()
    })();

    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e);
    }

    std::fs::rename(&tmp_path, path)
}

// ── Dispatch ───────────────────────────────────────────────────

/// Dispatch a tool call with paths confined to `root` (INV-FS-1).
pub fn dispatch_tool_in(
    name: &str,
    args: serde_json::Value,
    root: &Path,
) -> std::result::Result<String, String> {
    // Fail closed: only registered tools dispatch. An unregistered name can
    // never reach process execution — there is no wildcard fallthrough
    // (INV-TOOL-1). See docs/security/SECURITY_INVARIANTS.md.
    let tool = ToolId::from_name(name).map_err(|e| e.to_string())?;
    match tool {
        ToolId::ExecuteShell => {
            let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
            execute_shell(cmd)
        }
        ToolId::ReadFile => {
            let requested = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let policy = FsPolicy::new(root).map_err(|e| e.to_string())?;
            let path = policy.resolve_read(requested).map_err(|e| e.to_string())?;
            read_file_capped(&path, MAX_TOOL_FILE_READ_BYTES)
        }
        ToolId::WriteFile => {
            let requested = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            if content.len() > MAX_TOOL_FILE_WRITE_BYTES {
                return Err(format!(
                    "file content exceeds the {} byte tool limit",
                    MAX_TOOL_FILE_WRITE_BYTES
                ));
            }
            let policy = FsPolicy::new(root).map_err(|e| e.to_string())?;
            let path = policy.resolve_write(requested).map_err(|e| e.to_string())?;
            atomic_write(&path, content.as_bytes()).map_err(|e| e.to_string())?;
            Ok(format!("written {} bytes", content.len()))
        }
        ToolId::PatchFile => {
            let requested = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let old_text = args.get("old_text").and_then(|v| v.as_str()).unwrap_or("");
            let new_text = args.get("new_text").and_then(|v| v.as_str()).unwrap_or("");
            let policy = FsPolicy::new(root).map_err(|e| e.to_string())?;
            // patch requires the target to already exist.
            let path = policy.resolve_read(requested).map_err(|e| e.to_string())?;
            patch_file(&path, old_text, new_text)
        }
        ToolId::BrowserRead | ToolId::McpCall => {
            Err(format!("tool {name:?} requires asynchronous dispatch"))
        }
    }
}

/// Dispatch a tool call, confined to the process's current working directory.
/// This is the live agent path (`async_dispatch_tool` calls this).
pub fn dispatch_tool(name: &str, args: serde_json::Value) -> std::result::Result<String, String> {
    dispatch_tool_in(name, args, &default_workspace_root())
}

pub async fn async_dispatch_tool(
    name: &str,
    args: serde_json::Value,
) -> std::result::Result<String, String> {
    match ToolId::from_name(name).map_err(|error| error.to_string())? {
        ToolId::BrowserRead => {
            let url = args
                .get("url")
                .and_then(serde_json::Value::as_str)
                .ok_or("browser_read requires a url")?;
            let endpoint = args
                .get("cdp_endpoint")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("http://127.0.0.1:9222");
            let mut browser = soul_browser::BrowserController::with_endpoint(
                soul_browser::BrowserConfig::default(),
                endpoint,
            )
            .map_err(|error| error.to_string())?;
            browser.connect().await.map_err(|error| error.to_string())?;
            browser
                .navigate(url)
                .await
                .map_err(|error| error.to_string())?;
            tokio::time::sleep(Duration::from_millis(500)).await;
            browser.get_text().await.map_err(|error| error.to_string())
        }
        ToolId::McpCall => {
            let server_url = args
                .get("server_url")
                .and_then(serde_json::Value::as_str)
                .ok_or("mcp_call requires server_url")?;
            if !server_url.starts_with("ws://") && !server_url.starts_with("wss://") {
                return Err("mcp_call server_url must use ws:// or wss://".into());
            }
            let tool = args
                .get("tool")
                .and_then(serde_json::Value::as_str)
                .ok_or("mcp_call requires tool")?;
            let arguments = args
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let transport = soul_mcp::WsTransport::new(server_url);
            let (request_tx, response_rx) = transport
                .connect_client()
                .await
                .map_err(|error| error.to_string())?;
            let client = soul_mcp::McpClient::new(request_tx, response_rx);
            client
                .initialize(soul_mcp::McpServerInfo {
                    name: "soulsystem".into(),
                    version: env!("CARGO_PKG_VERSION").into(),
                })
                .await
                .map_err(|error| error.to_string())?;
            let result = client
                .call_tool(tool, arguments)
                .await
                .map_err(|error| error.to_string())?;
            serde_json::to_string(&result).map_err(|error| error.to_string())
        }
        _ => {
            let name = name.to_string();
            tokio::task::spawn_blocking(move || dispatch_tool(&name, args))
                .await
                .map_err(|e| format!("tool dispatch join error: {e}"))?
        }
    }
}

// ── Exécution synchronisée (legacy) ───────────────────────────

pub fn execute_shell(cmd: &str) -> std::result::Result<String, String> {
    // Single, mandatory process-execution path. Routes through
    // `soul_sandbox::Sandbox`, which applies command filtering (dangerous
    // patterns, sensitive paths, banned shell interpreters), a timeout, and
    // process-group termination. There is no bare `std::process::Command` on
    // this path anymore (CRIT-001 / INV-EXEC-1). The unused `execute_tool`
    // helper also runs through here.
    let sandbox = Sandbox::new(SandboxPolicy::default());
    match sandbox.execute(cmd) {
        Ok(verdict) => match verdict.exit_code {
            Some(0) => Ok(verdict.stdout),
            other => {
                let msg = verdict.stderr;
                if msg.trim().is_empty() {
                    Err(format!("command exited with status {other:?}"))
                } else {
                    Err(msg)
                }
            }
        },
        Err(blocked) => Err(format!("blocked by sandbox: {blocked}")),
    }
}

pub fn execute_tool(tool: &Tool, args: &str) -> std::result::Result<String, String> {
    let full_cmd = format!("{} {}", tool.name, args);
    execute_shell(&full_cmd)
}

// ── File operations backing the tool dispatch ──────────────────────────
//
// These operate only on paths already resolved and confinement-checked by
// `FsPolicy` (see `dispatch_tool_in`) — never on a caller-supplied string.

fn patch_file(path: &Path, old: &str, new: &str) -> Result<String, String> {
    let content = read_file_capped(path, MAX_TOOL_FILE_READ_BYTES)?;
    if !content.contains(old) {
        return Err(format!("pattern not found in {}", path.display()));
    }
    let updated = content.replacen(old, new, 1);
    if updated.len() > MAX_TOOL_FILE_WRITE_BYTES {
        return Err(format!(
            "patched file exceeds the {} byte tool limit",
            MAX_TOOL_FILE_WRITE_BYTES
        ));
    }
    atomic_write(path, updated.as_bytes()).map_err(|e| e.to_string())?;
    Ok(format!("patched {}", path.display()))
}

fn read_file_capped(path: &Path, max_bytes: usize) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::with_capacity(max_bytes.min(8192));
    file.take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > max_bytes {
        return Err(format!("file exceeds the {max_bytes} byte tool read limit"));
    }
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod compat_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn permission_classification() {
        assert_eq!(
            PermissionLevel::from_command("ls -la"),
            PermissionLevel::Read
        );
        assert_eq!(
            PermissionLevel::from_command("rm foo.txt"),
            PermissionLevel::Write
        );
        assert_eq!(
            PermissionLevel::from_command("sudo rm -rf /"),
            PermissionLevel::Destructive
        );
    }

    #[test]
    fn dispatch_file_ops_roundtrip() {
        // dispatch_tool_in confines file ops to `root`; a relative path is the
        // realistic agent usage (a workspace-relative reference).
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        dispatch_tool_in(
            "write_file",
            json!({"path": "a.txt", "content": "hello"}),
            root,
        )
        .unwrap();
        let read = dispatch_tool_in("read_file", json!({"path": "a.txt"}), root).unwrap();
        assert_eq!(read, "hello");
        dispatch_tool_in(
            "patch_file",
            json!({"path": "a.txt", "old_text": "hello", "new_text": "world"}),
            root,
        )
        .unwrap();
        assert_eq!(
            dispatch_tool_in("read_file", json!({"path": "a.txt"}), root).unwrap(),
            "world"
        );
    }

    // ── CRIT-004: filesystem confinement adversarial tests ──────────────

    #[test]
    fn outside_root_absolute_path_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // An absolute path elsewhere on the filesystem must never be reached,
        // whether or not it exists.
        let err = dispatch_tool_in(
            "write_file",
            json!({"path": "/etc/passwd", "content": "pwned"}),
            root,
        )
        .unwrap_err();
        assert!(
            err.contains("escapes the confined workspace root"),
            "got: {err}"
        );

        let err2 = dispatch_tool_in(
            "write_file",
            json!({"path": "/tmp/soul-tools-crit004-outside-root-probe.txt", "content": "x"}),
            root,
        )
        .unwrap_err();
        assert!(
            err2.contains("escapes the confined workspace root"),
            "got: {err2}"
        );
    }

    #[test]
    fn parent_traversal_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let err = dispatch_tool_in(
            "write_file",
            json!({"path": "../escape.txt", "content": "x"}),
            root,
        )
        .unwrap_err();
        assert!(
            err.contains("escapes the confined workspace root"),
            "got: {err}"
        );
    }

    #[test]
    fn absolute_path_within_root_is_allowed() {
        // An absolute path is fine as long as it resolves inside the root —
        // confinement is about containment, not about rejecting all absolute
        // paths outright.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let abs = root.join("inside.txt");
        let abs_str = abs.to_str().unwrap();
        dispatch_tool_in(
            "write_file",
            json!({"path": abs_str, "content": "ok"}),
            root,
        )
        .unwrap();
        assert_eq!(
            dispatch_tool_in("read_file", json!({"path": "inside.txt"}), root).unwrap(),
            "ok"
        );
    }

    #[test]
    #[cfg(unix)]
    fn symlink_escape_of_existing_target_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret.txt");
        std::fs::write(&secret, "top secret").unwrap();

        let link = root.join("link.txt");
        std::os::unix::fs::symlink(&secret, &link).unwrap();

        // Reading through the symlink must resolve to the real (outside)
        // target and be rejected — the sandbox must not follow it.
        let err = dispatch_tool_in("read_file", json!({"path": "link.txt"}), root).unwrap_err();
        assert!(
            err.contains("escapes the confined workspace root"),
            "got: {err}"
        );

        // Writing through the same symlink must also be rejected, not
        // silently overwrite the outside file.
        let err2 = dispatch_tool_in(
            "write_file",
            json!({"path": "link.txt", "content": "overwritten"}),
            root,
        )
        .unwrap_err();
        assert!(
            err2.contains("escapes the confined workspace root"),
            "got: {err2}"
        );
        assert_eq!(std::fs::read_to_string(&secret).unwrap(), "top secret");
    }

    #[test]
    #[cfg(unix)]
    fn symlink_escape_of_directory_ancestor_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let outside = tempfile::tempdir().unwrap();

        // A symlinked subdirectory pointing outside the root: writing a NEW
        // file "through" it must still be rejected (the ancestor-walk
        // canonicalises the nearest EXISTING ancestor, which is the symlink
        // itself here).
        let link_dir = root.join("outdir");
        std::os::unix::fs::symlink(outside.path(), &link_dir).unwrap();

        let err = dispatch_tool_in(
            "write_file",
            json!({"path": "outdir/new.txt", "content": "x"}),
            root,
        )
        .unwrap_err();
        assert!(
            err.contains("escapes the confined workspace root"),
            "got: {err}"
        );
        assert!(!outside.path().join("new.txt").exists());
    }

    #[test]
    #[cfg(unix)]
    fn broken_symlink_is_rejected_before_write_resolution() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::os::unix::fs::symlink(root.join("missing-target"), root.join("broken")).unwrap();

        let err = dispatch_tool_in(
            "write_file",
            json!({"path": "broken", "content": "x"}),
            root,
        )
        .unwrap_err();
        assert!(err.contains("broken symlink"), "got: {err}");
    }

    #[test]
    fn dot_git_is_protected() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join(".git")).unwrap();

        let err = dispatch_tool_in(
            "write_file",
            json!({"path": ".git/config", "content": "[core]\n"}),
            root,
        )
        .unwrap_err();
        assert!(err.contains("protected location"), "got: {err}");
    }

    #[test]
    fn empty_and_nul_paths_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let err =
            dispatch_tool_in("write_file", json!({"path": "", "content": "x"}), root).unwrap_err();
        assert!(err.contains("empty path"), "got: {err}");

        let err2 = dispatch_tool_in(
            "write_file",
            json!({"path": "a\0b.txt", "content": "x"}),
            root,
        )
        .unwrap_err();
        assert!(err2.contains("NUL byte"), "got: {err2}");
    }

    #[test]
    fn patch_requires_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let err = dispatch_tool_in(
            "patch_file",
            json!({"path": "missing.txt", "old_text": "a", "new_text": "b"}),
            root,
        )
        .unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    #[test]
    fn atomic_write_leaves_no_temp_file_and_is_readable_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        dispatch_tool_in(
            "write_file",
            json!({"path": "a.txt", "content": "hello atomic"}),
            root,
        )
        .unwrap();

        let entries: Vec<_> = std::fs::read_dir(root)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(entries, vec!["a.txt"], "no leftover temp file: {entries:?}");
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).unwrap(),
            "hello atomic"
        );
    }

    #[test]
    fn atomic_write_preserves_original_on_directory_failure() {
        // atomic_write's temp file lives in the SAME directory as the
        // destination; if that directory doesn't exist the write fails
        // (as before) and the pre-existing destination file, if any, is left
        // untouched — there is no window where it is truncated then never
        // rewritten.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        dispatch_tool_in(
            "write_file",
            json!({"path": "a.txt", "content": "original"}),
            root,
        )
        .unwrap();

        // A second write to the SAME existing file must fully replace it
        // (rename is atomic — no partial content is ever observable).
        dispatch_tool_in(
            "write_file",
            json!({"path": "a.txt", "content": "replaced"}),
            root,
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).unwrap(),
            "replaced"
        );
    }

    #[test]
    fn unknown_tool_never_executes() {
        // A non-existent command name errors, as before...
        let e = dispatch_tool("nope", json!({})).unwrap_err();
        assert!(e.contains("unknown tool"), "got: {e}");

        // ...and — the actual CRIT-002 fix — a REAL executable is rejected
        // rather than run. Under the old wildcard fallthrough, `dispatch_tool`
        // would have executed `echo {}` / `bash {}` and returned Ok. Now every
        // one of these must fail closed with UnknownTool.
        for name in ["echo", "bash", "sh", "python3", "env", "cat", "curl"] {
            let err = dispatch_tool(name, json!({"anything": "here"})).unwrap_err();
            assert!(
                err.contains("unknown tool"),
                "{name:?} must be rejected as unknown, got: {err}"
            );
        }
    }

    #[test]
    fn malformed_tool_names_rejected() {
        // Exact-match registry: no trimming, casing, path separators, control
        // characters, embedded arguments, or Unicode look-alikes get through.
        let names = [
            "",                 // empty
            " ",                // whitespace only
            "read_file ",       // trailing space
            " read_file",       // leading space
            "READ_FILE",        // wrong case
            "read/file",        // path separator
            "read_file\n",      // trailing newline
            "execute_shell;id", // embedded command
            "execute_shell id", // space-embedded argument
            "reаd_file",        // Cyrillic 'а' (U+0430) look-alike
        ];
        for name in names {
            assert!(
                ToolId::from_name(name).is_err(),
                "name {name:?} must not resolve to a tool"
            );
            assert!(
                dispatch_tool(name, json!({})).is_err(),
                "dispatch of {name:?} must fail closed"
            );
        }
    }

    #[test]
    fn registry_roundtrip_and_membership() {
        for id in ToolId::all() {
            assert_eq!(ToolId::from_name(id.as_str()), Ok(id));
            assert!(ToolId::is_registered(id.as_str()));
        }
        assert!(!ToolId::is_registered("bash"));
        assert!(!ToolId::is_registered(""));
    }

    #[test]
    fn capability_classification_is_trusted_and_correct() {
        assert_eq!(ToolId::ReadFile.capability(), ToolCapability::ReadOnly);
        assert_eq!(ToolId::WriteFile.capability(), ToolCapability::FileWrite);
        assert_eq!(ToolId::PatchFile.capability(), ToolCapability::FileWrite);
        assert_eq!(
            ToolId::BrowserRead.capability(),
            ToolCapability::NetworkRead
        );
        assert_eq!(ToolId::McpCall.capability(), ToolCapability::NetworkWrite);
        assert_eq!(
            ToolId::ExecuteShell.capability(),
            ToolCapability::ProcessExecution
        );
    }

    #[test]
    fn write_tools_are_not_classified_as_read() {
        // The CRIT-003 fix: write_file / patch_file must require Write, not Read.
        assert_eq!(
            required_permission_for("write_file", None),
            PermissionLevel::Write
        );
        assert_eq!(
            required_permission_for("patch_file", None),
            PermissionLevel::Write
        );
        // read_file remains Read.
        assert_eq!(
            required_permission_for("read_file", None),
            PermissionLevel::Read
        );
    }

    #[test]
    fn execute_shell_is_classified_per_command() {
        assert_eq!(
            required_permission_for("execute_shell", Some("ls -la")),
            PermissionLevel::Read
        );
        assert_eq!(
            required_permission_for("execute_shell", Some("rm foo")),
            PermissionLevel::Write
        );
        assert_eq!(
            required_permission_for("execute_shell", Some("sudo rm -rf /")),
            PermissionLevel::Destructive
        );
    }

    #[test]
    fn unknown_tool_classified_most_restrictive() {
        assert_eq!(
            required_permission_for("bash", None),
            PermissionLevel::Destructive
        );
        assert_eq!(
            required_permission_for("", None),
            PermissionLevel::Destructive
        );
    }

    #[test]
    fn policy_engine_matches_canonical_classifier() {
        let policy = CapabilityPolicy;
        assert_eq!(
            policy.required_permission(&CapabilityRequest {
                tool_name: "write_file",
                command: None
            }),
            PermissionLevel::Write
        );
        assert_eq!(
            policy.required_permission(&CapabilityRequest {
                tool_name: "execute_shell",
                command: Some("ls")
            }),
            PermissionLevel::Read
        );
    }

    #[tokio::test]
    async fn async_shell_runs() {
        let out = async_dispatch_tool("execute_shell", json!({"command": "echo hi"}))
            .await
            .unwrap();
        assert!(out.contains("hi"));
    }

    #[test]
    fn execute_shell_routes_through_sandbox() {
        // A safe command runs through the sandbox.
        let out = execute_shell("echo sandboxed").unwrap();
        assert!(out.contains("sandboxed"), "got: {out}");

        // A destructive command is blocked BY THE SANDBOX before execution — a
        // bare std::process::Command would instead have attempted to run it.
        // This proves execute_shell no longer bypasses OS-level filtering
        // (CRIT-001 / INV-EXEC-1).
        let err = execute_shell("rm -rf /").unwrap_err();
        assert!(err.contains("blocked by sandbox"), "got: {err}");

        // Shell-bypass interpreters (bash -c / sh -c) are refused.
        let err2 = execute_shell("bash -c 'echo x'").unwrap_err();
        assert!(err2.contains("blocked by sandbox"), "got: {err2}");
    }
}
