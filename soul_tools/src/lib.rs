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
use std::process::Command;

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

    /// All registered tools as borrowed references (agent-facing alias).
    pub fn list(&self) -> Vec<&Tool> {
        self.tools.iter().collect()
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

// ── Exécution ────────────────────────────────────────────────

pub fn execute_shell(cmd: &str) -> Result<String, String> {
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

pub fn execute_tool(tool: &Tool, args: &str) -> Result<String, String> {
    let full_cmd = format!("{} {}", tool.name, args);
    execute_shell(&full_cmd)
}

// ════════════════════════════════════════════════════════════════════════
//  Compat layer for the autonomous ReAct agent (`soul_agent_core`)
//
//  Restores the permission classification, async shell executor, and tool
//  dispatch surface the agent's loop depends on. Additive — the minimal
//  discovery/registry API above is untouched.
// ════════════════════════════════════════════════════════════════════════

use serde_json::Value;
use std::time::Duration;

/// How dangerous a command is, used to gate tool execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionLevel {
    /// Read-only (ls, cat, grep…).
    Read,
    /// Mutates state (write, mv, rm of a file, mkdir…).
    Write,
    /// Irreversible / system-wide damage (rm -rf /, mkfs, dd to a device…).
    Destructive,
}

impl PermissionLevel {
    /// Classify a shell command conservatively: when in doubt, escalate.
    pub fn from_command(cmd: &str) -> Self {
        let c = cmd.to_lowercase();
        const DESTRUCTIVE: &[&str] = &[
            "rm -rf /",
            "rm -fr /",
            "mkfs",
            "dd if=",
            ":(){",
            "shutdown",
            "reboot",
            "> /dev/sd",
            "chmod -r 000 /",
            "wipefs",
            "fdisk",
        ];
        if DESTRUCTIVE.iter().any(|p| c.contains(p)) {
            return PermissionLevel::Destructive;
        }
        const WRITE: &[&str] = &[
            "rm ",
            "rmdir",
            "mv ",
            "cp ",
            "touch ",
            "mkdir",
            "tee ",
            "install",
            ">",
            ">>",
            "truncate",
            "sed -i",
            "patch",
            "git commit",
            "git push",
            "git reset",
            "chmod",
            "chown",
        ];
        if WRITE.iter().any(|p| c.contains(p)) {
            return PermissionLevel::Write;
        }
        PermissionLevel::Read
    }
}

/// Captured output of a sandboxed shell command.
#[derive(Debug, Clone)]
pub struct ShellOutput {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
    pub timed_out: bool,
}

/// Runs shell commands asynchronously with a wall-clock timeout.
pub struct AsyncShellExecutor {
    timeout: Duration,
}

impl AsyncShellExecutor {
    pub fn new(timeout_secs: u64) -> Self {
        Self {
            timeout: Duration::from_secs(timeout_secs.max(1)),
        }
    }

    pub async fn execute(&self, command: &str) -> Result<ShellOutput, String> {
        let fut = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .output();
        match tokio::time::timeout(self.timeout, fut).await {
            Ok(Ok(out)) => Ok(ShellOutput {
                stdout: String::from_utf8_lossy(&out.stdout).to_string(),
                stderr: String::from_utf8_lossy(&out.stderr).to_string(),
                success: out.status.success(),
                timed_out: false,
            }),
            Ok(Err(e)) => Err(format!("spawn failed: {e}")),
            Err(_) => Ok(ShellOutput {
                stdout: String::new(),
                stderr: format!("command timed out after {:?}", self.timeout),
                success: false,
                timed_out: true,
            }),
        }
    }
}

// ── File operations backing the tool dispatch ──────────────────────────

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
    let patched = content.replace(old, new);
    std::fs::write(path, &patched).map_err(|e| e.to_string())?;
    Ok(format!("patched {path}"))
}

fn list_directory(path: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        out.push(entry.file_name().to_string_lossy().to_string());
    }
    out.sort();
    Ok(out)
}

fn walk(root: &std::path::Path, out: &mut Vec<std::path::PathBuf>, limit: usize) {
    if out.len() >= limit {
        return;
    }
    let Ok(rd) = std::fs::read_dir(root) else {
        return;
    };
    for entry in rd.flatten() {
        if out.len() >= limit {
            return;
        }
        let p = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "target" || name == ".git" || name == "node_modules" {
            continue;
        }
        if p.is_dir() {
            walk(&p, out, limit);
        } else {
            out.push(p);
        }
    }
}

fn search_files(pattern: &str, path: &str) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    walk(std::path::Path::new(path), &mut files, 5000);
    Ok(files
        .into_iter()
        .filter(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().contains(pattern))
                .unwrap_or(false)
        })
        .map(|p| p.display().to_string())
        .take(200)
        .collect())
}

fn grep_content(
    pattern: &str,
    path: &str,
    file_pattern: Option<&str>,
) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    walk(std::path::Path::new(path), &mut files, 5000);
    let mut hits = Vec::new();
    for f in files {
        if let Some(fp) = file_pattern {
            if !f
                .file_name()
                .map(|n| n.to_string_lossy().contains(fp))
                .unwrap_or(false)
            {
                continue;
            }
        }
        if let Ok(content) = std::fs::read_to_string(&f) {
            for (i, line) in content.lines().enumerate() {
                if line.contains(pattern) {
                    hits.push(format!("{}:{}: {}", f.display(), i + 1, line.trim()));
                    if hits.len() >= 200 {
                        return Ok(hits);
                    }
                }
            }
        }
    }
    Ok(hits)
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("missing '{key}' argument"))
}

/// Dispatch a named tool synchronously. Tool names mirror
/// `soul_llm::build_tool_schemas`.
pub fn dispatch_tool(tool_name: &str, args: &Value) -> Result<String, String> {
    match tool_name {
        "execute_shell" => execute_shell(arg_str(args, "command")?),
        "read_file" => {
            let start = args
                .get("start_line")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);
            let num = args
                .get("num_lines")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);
            read_file(arg_str(args, "path")?, start, num)
        }
        "write_file" => {
            let append = args.get("mode").and_then(|v| v.as_str()) == Some("append");
            write_file(arg_str(args, "path")?, arg_str(args, "content")?, append)?;
            Ok(format!("written to {}", arg_str(args, "path")?))
        }
        "patch_file" => patch_file(
            arg_str(args, "path")?,
            arg_str(args, "old_text")?,
            arg_str(args, "new_text")?,
        ),
        "list_directory" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            Ok(list_directory(path)?.join("\n"))
        }
        "search_files" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            Ok(search_files(arg_str(args, "pattern")?, path)?.join("\n"))
        }
        "grep_content" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let fp = args.get("file_pattern").and_then(|v| v.as_str());
            Ok(grep_content(arg_str(args, "pattern")?, path, fp)?.join("\n"))
        }
        other => Err(format!("unknown tool: {other}")),
    }
}

/// Async dispatch. `execute_shell` runs with a timeout; the rest reuse the
/// synchronous file operations.
pub async fn async_dispatch_tool(tool_name: &str, args: Value) -> Result<String, String> {
    if tool_name == "execute_shell" {
        let cmd = arg_str(&args, "command")?.to_string();
        let exec = AsyncShellExecutor::new(60);
        let out = exec.execute(&cmd).await?;
        return if out.success {
            Ok(out.stdout)
        } else if out.timed_out {
            Err(out.stderr)
        } else {
            Err(if out.stderr.is_empty() {
                out.stdout
            } else {
                out.stderr
            })
        };
    }
    dispatch_tool(tool_name, &args)
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
        dispatch_tool("write_file", &json!({"path": fp, "content": "hello"})).unwrap();
        let read = dispatch_tool("read_file", &json!({"path": fp})).unwrap();
        assert_eq!(read, "hello");
        dispatch_tool(
            "patch_file",
            &json!({"path": fp, "old_text": "hello", "new_text": "world"}),
        )
        .unwrap();
        assert_eq!(
            dispatch_tool("read_file", &json!({"path": fp})).unwrap(),
            "world"
        );
    }

    #[test]
    fn unknown_tool_errors() {
        assert!(dispatch_tool("nope", &json!({})).is_err());
    }

    #[tokio::test]
    async fn async_shell_runs() {
        let out = async_dispatch_tool("execute_shell", json!({"command": "echo hi"}))
            .await
            .unwrap();
        assert!(out.contains("hi"));
    }
}
