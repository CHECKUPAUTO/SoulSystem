// ==========================================================================
// soulsystem_bridge.rs — Pont HTTP ChronosAgent → SoulSystem
//
// Sauvegarde périodique du checkpoint directement vers SoulSystem
// via POST multipart, ce qui ferme la dernière flèche manquante du schéma
// architectural : ChronosAgent → checkpoint → SoulSystem → soul-memory.
//
// Usage :
//   let bridge = SoulSystemBridge::new("http://localhost:9023")?;
//   bridge.store(ckpt_dir, "agent_v6.3")?;
// ==========================================================================

use std::fs;
use std::path::Path;
use std::io::Read;

const DEFAULT_SOULSYSTEM_URL: &str = "http://localhost:9023";
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Pont HTTP vers SoulSystem pour persistance distante des checkpoints.
pub struct SoulSystemBridge {
    base_url: String,
    timeout_secs: u64,
}

impl SoulSystemBridge {
    /// Crée un nouveau bridge vers l'URL de SoulSystem.
    /// L'URL par défaut est http://localhost:9023.
    pub fn new(url: Option<&str>) -> Self {
        Self {
            base_url: url.unwrap_or(DEFAULT_SOULSYSTEM_URL).trim_end_matches('/').to_string(),
            timeout_secs: DEFAULT_TIMEOUT_SECS,
        }
    }

    /// Configure le timeout HTTP.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Upload le checkpoint complet (manifest + safetensors + replay + memory)
    /// vers SoulSystem via multipart POST.
    ///
    /// Retourne Ok(true) si le serveur a accepté, Ok(false) si le checkpoing
    /// n'existe pas localement, Err si la requête a échoué.
    pub fn store(&self, checkpoint_dir: &str, agent_id: &str) -> Result<bool, String> {
        let ckpt_path = Path::new(checkpoint_dir);
        if !ckpt_path.exists() {
            return Ok(false);
        }

        // Construire le formulaire multipart avec reqwest
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(self.timeout_secs))
            .build()
            .map_err(|e| format!("client build: {}", e))?;

        let url = format!("{}/memory/store", self.base_url);

        let mut form = reqwest::blocking::multipart::Form::new()
            .text("agent_id", agent_id.to_string())
            .text("format_version", "0.3.0")
            .text("timestamp", chrono::Utc::now().to_rfc3339());

        // Ajouter chaque fichier du checkpoint
        let files = ["manifest.json", "weights.safetensors", "replay.json", "atemporal_memory.json"];
        for filename in &files {
            let filepath = ckpt_path.join(filename);
            if filepath.exists() {
                let mut buf = Vec::new();
                fs::File::open(&filepath)
                    .and_then(|mut f| f.read_to_end(&mut buf))
                    .map_err(|e| format!("read {}: {}", filename, e))?;
                let part = reqwest::blocking::multipart::Part::bytes(buf)
                    .file_name(filename.to_string())
                    .mime_str("application/octet-stream")
                    .map_err(|e| format!("mime {}: {}", filename, e))?;
                form = form.part(filename.to_string(), part);
            }
        }

        let resp = client.post(&url)
            .multipart(form)
            .send()
            .map_err(|e| format!("POST {}: {}", url, e))?;

        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        if status.is_success() {
            println!("  🌐 SoulSystem store OK ({}): {}", status, body.trim());
            Ok(true)
        } else {
            Err(format!("SoulSystem store failed: {} {}", status, body.trim()))
        }
    }

    /// Vérifie si le serveur SoulSystem est joignable.
    pub fn health_check(&self) -> Result<bool, String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|e| format!("client build: {}", e))?;

        let url = format!("{}/health", self.base_url);
        let resp = client.get(&url)
            .send()
            .map_err(|e| format!("GET {}: {}", url, e))?;

        Ok(resp.status().is_success())
    }

    /// Retourne l'URL de base du serveur SoulSystem.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}
