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
