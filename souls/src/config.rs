use anyhow::{Context, Result};
use colored::Colorize;
use dialoguer::{Input, Password, Select};
use std::fs;
use std::path::PathBuf;

const CONFIG_DIR: &str = "soulsystem";
const CONFIG_FILE: &str = "config.toml";

/// Configuration persistante du LLM.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LlmPersistConfig {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub temperature: f32,
    pub max_tokens: usize,
}

impl Default for LlmPersistConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".into(),
            base_url: "http://127.0.0.1:11434".into(),
            model: "qwen3:8b".into(),
            api_key: None,
            temperature: 0.7,
            max_tokens: 2048,
        }
    }
}

/// Configuration globale persistante.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PersistConfig {
    pub llm: LlmPersistConfig,
    pub entity_name: String,
    pub gateway_addr: String,
    pub autonomous: bool,
}

impl Default for PersistConfig {
    fn default() -> Self {
        Self {
            llm: LlmPersistConfig::default(),
            entity_name: "soul".into(),
            gateway_addr: "127.0.0.1:7878".into(),
            autonomous: false,
        }
    }
}

/// Chemin du fichier de config.
fn config_path() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .context("impossible de déterminer le répertoire de config (~/.config)")?
        .join(CONFIG_DIR);
    Ok(dir.join(CONFIG_FILE))
}

/// Charger la config depuis le fichier, ou créer une config par défaut.
pub fn load_config() -> PersistConfig {
    let path = match config_path() {
        Ok(p) => p,
        Err(_) => return PersistConfig::default(),
    };
    match fs::read_to_string(&path) {
        Ok(content) => toml::from_str(&content).unwrap_or_default(),
        Err(_) => PersistConfig::default(),
    }
}

/// Sauvegarder la config sur disque.
pub fn save_config(cfg: &PersistConfig) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("impossible de créer {}", parent.display()))?;
    }
    let content = toml::to_string_pretty(cfg).context("erreur sérialisation TOML")?;
    fs::write(&path, content)
        .with_context(|| format!("impossible d'écrire {}", path.display()))?;
    Ok(())
}

/// Menu interactif de configuration.
pub fn run_config_menu(existing: &PersistConfig) -> Result<PersistConfig> {
    let mut cfg = existing.clone();

    println!(
        "\n{}",
        "╔══════════════════════════════════════════════════╗"
            .bright_cyan()
            .bold()
    );
    println!(
        "{}",
        "║   SoulSystem — Configuration                    ║"
            .bright_cyan()
            .bold()
    );
    println!(
        "{}",
        "╚══════════════════════════════════════════════════╝"
            .bright_cyan()
            .bold()
    );

    loop {
        println!();
        let items = vec![
            format!("Provider LLM    : {}", cfg.llm.provider.bright_green()),
            format!("URL du serveur  : {}", cfg.llm.base_url.bright_green()),
            format!("Modèle          : {}", cfg.llm.model.bright_green()),
            format!(
                "Clé API         : {}",
                if cfg.llm.api_key.is_some() {
                    "*** configurée ***".bright_yellow().to_string()
                } else {
                    "non définie".bright_red().to_string()
                }
            ),
            format!("Température     : {:.1}", cfg.llm.temperature),
            format!("Max tokens      : {}", cfg.llm.max_tokens),
            "---".to_string(),
            format!("Nom entité      : {}", cfg.entity_name.bright_green()),
            format!("Gateway         : {}", cfg.gateway_addr.bright_green()),
            format!(
                "Autonome        : {}",
                if cfg.autonomous { "oui" } else { "non" }
            ),
            "---".to_string(),
            "Sauvegarder et quitter".bright_green().bold().to_string(),
            "Quitter sans sauvegarder".bright_red().to_string(),
        ];

        let selection = Select::new()
            .with_prompt("Que souhaitez-vous configurer ?")
            .items(&items)
            .default(0)
            .interact_opt()?;

        match selection {
            Some(0) => configure_provider(&mut cfg)?,
            Some(1) => configure_url(&mut cfg)?,
            Some(2) => configure_model(&mut cfg)?,
            Some(3) => configure_api_key(&mut cfg)?,
            Some(4) => configure_temperature(&mut cfg)?,
            Some(5) => configure_max_tokens(&mut cfg)?,
            Some(7) => configure_entity_name(&mut cfg)?,
            Some(8) => configure_gateway(&mut cfg)?,
            Some(9) => configure_autonomous(&mut cfg)?,
            Some(11) => {
                save_config(&cfg)?;
                println!(
                    "\n{} Configuration sauvegardée dans {}",
                    "✓".bright_green().bold(),
                    config_path()?.display()
                );
                return Ok(cfg);
            }
            Some(12) => {
                println!("\n{}", "Abandonné.".bright_yellow());
                return Ok(cfg);
            }
            _ => {}
        }
    }
}

fn configure_provider(cfg: &mut PersistConfig) -> Result<()> {
    let providers = ["ollama", "openai", "anthropic"];
    let idx = Select::new()
        .with_prompt("Provider LLM")
        .items(&providers)
        .default(
            providers
                .iter()
                .position(|p| *p == cfg.llm.provider)
                .unwrap_or(0),
        )
        .interact_opt()?;

    if let Some(i) = idx {
        cfg.llm.provider = providers[i].to_string();

        // URL par défaut selon le provider
        cfg.llm.base_url = match providers[i] {
            "ollama" => "http://127.0.0.1:11434".into(),
            "openai" => "https://api.openai.com".into(),
            "anthropic" => "https://api.anthropic.com".into(),
            _ => cfg.llm.base_url.clone(),
        };

        // Modèles suggérés
        let models = match providers[i] {
            "ollama" => vec!["qwen3:8b", "llama3.1:8b", "mistral:latest", "deepseek-r1:8b"],
            "openai" => vec!["gpt-4o", "gpt-4o-mini", "gpt-4.1", "o4-mini"],
            "anthropic" => vec![
                "claude-sonnet-4-20250514",
                "claude-haiku-4-20250414",
                "claude-opus-4-20250514",
            ],
            _ => vec![],
        };

        if !models.is_empty() {
            let model_idx = Select::new()
                .with_prompt("Modèle suggéré")
                .items(&models)
                .default(0)
                .interact_opt()?;
            if let Some(m) = model_idx {
                cfg.llm.model = models[m].to_string();
            }
        }
    }
    Ok(())
}

fn configure_url(cfg: &mut PersistConfig) -> Result<()> {
    let url: String = Input::new()
        .with_prompt("URL du serveur LLM")
        .default(cfg.llm.base_url.clone())
        .interact_text()?;
    cfg.llm.base_url = url;
    Ok(())
}

fn configure_model(cfg: &mut PersistConfig) -> Result<()> {
    let model: String = Input::new()
        .with_prompt("Nom du modèle")
        .default(cfg.llm.model.clone())
        .interact_text()?;
    cfg.llm.model = model;
    Ok(())
}

fn configure_api_key(cfg: &mut PersistConfig) -> Result<()> {
    let key: String = Password::new()
        .with_prompt("Clé API (vide pour supprimer)")
        .with_confirmation("Confirmer", "Les clés ne correspondent pas")
        .allow_empty_password(true)
        .interact()?;

    cfg.llm.api_key = if key.is_empty() {
        None
    } else {
        Some(key)
    };
    Ok(())
}

fn configure_temperature(cfg: &mut PersistConfig) -> Result<()> {
    let val: String = Input::new()
        .with_prompt("Température (0.0 — 2.0)")
        .default(format!("{:.1}", cfg.llm.temperature))
        .interact_text()?;

    if let Ok(t) = val.parse::<f32>() {
        let clamped = t.clamp(0.0, 2.0);
        cfg.llm.temperature = clamped;
    } else {
        println!("{}", "Valeur invalide, conservation de la précédente.".bright_red());
    }
    Ok(())
}

fn configure_max_tokens(cfg: &mut PersistConfig) -> Result<()> {
    let val: String = Input::new()
        .with_prompt("Max tokens (1 — 128000)")
        .default(cfg.llm.max_tokens.to_string())
        .interact_text()?;

    if let Ok(t) = val.parse::<usize>() {
        let clamped = t.clamp(1, 128_000);
        cfg.llm.max_tokens = clamped;
    } else {
        println!("{}", "Valeur invalide, conservation de la précédente.".bright_red());
    }
    Ok(())
}

fn configure_entity_name(cfg: &mut PersistConfig) -> Result<()> {
    let name: String = Input::new()
        .with_prompt("Nom de l'entité")
        .default(cfg.entity_name.clone())
        .interact_text()?;
    cfg.entity_name = name;
    Ok(())
}

fn configure_gateway(cfg: &mut PersistConfig) -> Result<()> {
    let addr: String = Input::new()
        .with_prompt("Adresse du gateway (host:port)")
        .default(cfg.gateway_addr.clone())
        .interact_text()?;
    cfg.gateway_addr = addr;
    Ok(())
}

fn configure_autonomous(cfg: &mut PersistConfig) -> Result<()> {
    let options = vec!["Désactivé", "Activé"];
    let idx = Select::new()
        .with_prompt("Mode autonome")
        .items(&options)
        .default(if cfg.autonomous { 1 } else { 0 })
        .interact_opt()?;

    if let Some(i) = idx {
        cfg.autonomous = i == 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let cfg = PersistConfig::default();
        assert_eq!(cfg.llm.provider, "ollama");
        assert_eq!(cfg.llm.temperature, 0.7_f32);
        assert!(!cfg.autonomous);
    }

    #[test]
    fn serialize_roundtrip() {
        let cfg = PersistConfig::default();
        let toml_str = toml::to_string_pretty(&cfg).unwrap();
        let parsed: PersistConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.llm.provider, cfg.llm.provider);
        assert_eq!(parsed.llm.model, cfg.llm.model);
    }
}
