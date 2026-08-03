use std::path::PathBuf;

/// Configuration globale du système SoulSystem v0.2
/// Lit les variables d'environnement si présentes, sinon valeurs par défaut
#[derive(Debug, Clone)]
pub struct Config {
    // ── Ollama ───────────────────────────────
    pub ollama_url: String,
    /// Modèle rapide pour mutations courantes
    pub model_fast: String,
    /// Modèle profond pour stagnation
    pub model_deep: String,
    /// Modèle d'embeddings pour ranking sémantique
    pub model_embed: String,

    // ── GPU RTX 4060 ─────────────────────────
    pub max_concurrent_fast: usize,
    pub max_concurrent_deep: usize,
    pub context_window_fast: usize,
    pub context_window_deep: usize,

    // ── Population ───────────────────────────
    pub population_size: usize,
    pub mutation_rate: f32,
    pub survival_rate: f32,

    // ── Stagnation ───────────────────────────
    /// Nombre de cycles sans amélioration avant bascule en mode DEEP
    pub stagnation_threshold: usize,
    /// Amélioration minimale de fitness pour considérer un progrès
    pub stagnation_min_improvement: f32,

    // ── Sandbox ──────────────────────────────
    /// Activer l'exécution sandbox (compile + run les code_patch)
    pub sandbox_enabled: bool,
    /// Timeout d'exécution sandbox en secondes
    pub sandbox_timeout_secs: u64,
    /// Répertoire temporaire pour les sandboxes
    pub sandbox_dir: PathBuf,

    // ── Système ──────────────────────────────
    pub snapshot_interval: usize,
    pub snapshot_dir: PathBuf,
    pub agents_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub max_crawl_results: usize,
    pub http_timeout_secs: u64,
    pub crawler_max_retries: u32,
    pub cycle_delay_secs: u64,
    pub seed_urls: Vec<String>,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            // Ollama multi-modèle
            ollama_url: env_or("SOULSYSTEM_EVO_OLLAMA_URL", "http://localhost:11434"),
            model_fast: env_or("SOULSYSTEM_EVO_MODEL_FAST", "qwen3.5:4b"),
            model_deep: env_or("SOULSYSTEM_EVO_MODEL_DEEP", "llama3:8b"),
            model_embed: env_or("SOULSYSTEM_EVO_MODEL_EMBED", "nomic-embed-text"),

            // GPU
            max_concurrent_fast: env_parse("SOULSYSTEM_EVO_MAX_CONCURRENT_FAST", 2),
            max_concurrent_deep: env_parse("SOULSYSTEM_EVO_MAX_CONCURRENT_DEEP", 1),
            context_window_fast: env_parse("SOULSYSTEM_EVO_CONTEXT_WINDOW_FAST", 2048),
            context_window_deep: env_parse("SOULSYSTEM_EVO_CONTEXT_WINDOW_DEEP", 4096),

            // Population
            population_size: env_parse("SOULSYSTEM_EVO_POPULATION_SIZE", 20),
            mutation_rate: env_parse("SOULSYSTEM_EVO_MUTATION_RATE", 0.3),
            survival_rate: env_parse("SOULSYSTEM_EVO_SURVIVAL_RATE", 0.5),

            // Stagnation
            stagnation_threshold: env_parse("SOULSYSTEM_EVO_STAGNATION_THRESHOLD", 10),
            stagnation_min_improvement: env_parse(
                "SOULSYSTEM_EVO_STAGNATION_MIN_IMPROVEMENT",
                0.005,
            ),

            // Sandbox
            sandbox_enabled: env_parse("SOULSYSTEM_EVO_SANDBOX_ENABLED", true),
            sandbox_timeout_secs: env_parse("SOULSYSTEM_EVO_SANDBOX_TIMEOUT", 30),
            sandbox_dir: PathBuf::from(env_or("SOULSYSTEM_EVO_SANDBOX_DIR", "./data/sandbox")),

            // Système
            snapshot_interval: env_parse("SOULSYSTEM_EVO_SNAPSHOT_INTERVAL", 5),
            snapshot_dir: PathBuf::from(env_or("SOULSYSTEM_EVO_SNAPSHOT_DIR", "./data/snapshots")),
            agents_dir: PathBuf::from(env_or("SOULSYSTEM_EVO_AGENTS_DIR", "./data/agents")),
            logs_dir: PathBuf::from(env_or("SOULSYSTEM_EVO_LOGS_DIR", "./logs")),
            max_crawl_results: env_parse("SOULSYSTEM_EVO_MAX_CRAWL_RESULTS", 20),
            http_timeout_secs: env_parse("SOULSYSTEM_EVO_HTTP_TIMEOUT", 30),
            crawler_max_retries: env_parse("SOULSYSTEM_EVO_CRAWLER_RETRIES", 3),
            cycle_delay_secs: env_parse("SOULSYSTEM_EVO_CYCLE_DELAY", 5),
            seed_urls: env_list(
                "SOULSYSTEM_EVO_SEED_URLS",
                vec![
                    "https://docs.rs".into(),
                    "https://crates.io".into(),
                    "https://arxiv.org/list/cs.AI/recent".into(),
                    "https://news.ycombinator.com".into(),
                ],
            ),
        }
    }

    pub fn display(&self) {
        tracing::info!("Configuration SoulSystem v0.2:");
        tracing::info!("  Ollama URL       : {}", self.ollama_url);
        tracing::info!(
            "  Modèle FAST      : {} (conc={}, ctx={})",
            self.model_fast,
            self.max_concurrent_fast,
            self.context_window_fast
        );
        tracing::info!(
            "  Modèle DEEP      : {} (conc={}, ctx={})",
            self.model_deep,
            self.max_concurrent_deep,
            self.context_window_deep
        );
        tracing::info!("  Modèle EMBED     : {}", self.model_embed);
        tracing::info!("  Population       : {}", self.population_size);
        tracing::info!(
            "  Mutation/Survie  : {:.0}% / {:.0}%",
            self.mutation_rate * 100.0,
            self.survival_rate * 100.0
        );
        tracing::info!(
            "  Stagnation seuil : {} cycles (min Δ={:.4})",
            self.stagnation_threshold,
            self.stagnation_min_improvement
        );
        tracing::info!(
            "  Sandbox          : {} (timeout={}s)",
            if self.sandbox_enabled { "ON" } else { "OFF" },
            self.sandbox_timeout_secs
        );
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_list(key: &str, default: Vec<String>) -> Vec<String> {
    match std::env::var(key) {
        Ok(val) => val.split(',').map(|s| s.trim().to_string()).collect(),
        Err(_) => default,
    }
}
