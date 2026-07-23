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
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

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
}

impl ToolId {
    /// The canonical wire name of this tool.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExecuteShell => "execute_shell",
            Self::ReadFile => "read_file",
            Self::WriteFile => "write_file",
            Self::PatchFile => "patch_file",
        }
    }

    /// Every registered tool id.
    pub fn all() -> [ToolId; 4] {
        [
            Self::ExecuteShell,
            Self::ReadFile,
            Self::WriteFile,
            Self::PatchFile,
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

// ── Dispatch ───────────────────────────────────────────────────

pub fn dispatch_tool(name: &str, args: serde_json::Value) -> std::result::Result<String, String> {
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
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            std::fs::read_to_string(path).map_err(|e| e.to_string())
        }
        ToolId::WriteFile => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            std::fs::write(path, content).map_err(|e| e.to_string())?;
            Ok(format!("written {} bytes", content.len()))
        }
        ToolId::PatchFile => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let old_text = args.get("old_text").and_then(|v| v.as_str()).unwrap_or("");
            let new_text = args.get("new_text").and_then(|v| v.as_str()).unwrap_or("");
            patch_file(path, old_text, new_text)
        }
    }
}

pub async fn async_dispatch_tool(
    name: &str,
    args: serde_json::Value,
) -> std::result::Result<String, String> {
    let name = name.to_string();
    tokio::task::spawn_blocking(move || dispatch_tool(&name, args))
        .await
        .map_err(|e| format!("tool dispatch join error: {e}"))?
}

// ── Exécution synchronisée (legacy) ───────────────────────────

pub fn execute_shell(cmd: &str) -> std::result::Result<String, String> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.is_empty() {
        return Err("command vide".into());
    }

    let output = Command::new(parts[0])
        .args(&parts[1..])
        .output()
        .map_err(|e| format!("échec exécution: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(stdout)
    } else {
        Err(stderr)
    }
}

pub fn execute_tool(tool: &Tool, args: &str) -> std::result::Result<String, String> {
    let full_cmd = format!("{} {}", tool.name, args);
    execute_shell(&full_cmd)
}

// ── File operations backing the tool dispatch ──────────────────────────

#[allow(dead_code)]
fn read_file(path: &str, start: Option<usize>, num: Option<usize>) -> Result<String, String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    match (start, num) {
        (None, None) => Ok(content),
        (s, n) => {
            let skip = s.unwrap_or(1).saturating_sub(1);
            let take = n.unwrap_or(usize::MAX);
            Ok(content
                .lines()
                .skip(skip)
                .take(take)
                .collect::<Vec<_>>()
                .join("\n"))
        }
    }
}

#[allow(dead_code)]
fn write_file(path: &str, content: &str, append: bool) -> Result<(), String> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(path)
        .map_err(|e| e.to_string())?;
    f.write_all(content.as_bytes()).map_err(|e| e.to_string())
}

fn patch_file(path: &str, old: &str, new: &str) -> Result<String, String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    if !content.contains(old) {
        return Err(format!("pattern not found in {path}"));
    }
    let updated = content.replacen(old, new, 1);
    std::fs::write(path, updated).map_err(|e| e.to_string())?;
    Ok(format!("patched {path}"))
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
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.txt");
        let fp = f.to_str().unwrap();
        dispatch_tool("write_file", json!({"path": fp, "content": "hello"})).unwrap();
        let read = dispatch_tool("read_file", json!({"path": fp})).unwrap();
        assert_eq!(read, "hello");
        dispatch_tool(
            "patch_file",
            json!({"path": fp, "old_text": "hello", "new_text": "world"}),
        )
        .unwrap();
        assert_eq!(
            dispatch_tool("read_file", json!({"path": fp})).unwrap(),
            "world"
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
}
