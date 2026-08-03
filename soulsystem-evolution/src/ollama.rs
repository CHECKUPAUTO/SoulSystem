use crate::agent::{Agent, Mutation, MutationType};
use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{info, warn};

// ─────────────────────────────────────────────
// Structures API Ollama
// ─────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct OllamaGenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    num_ctx: usize,
    temperature: f32,
    top_p: f32,
}

#[derive(Debug, Deserialize)]
struct OllamaGenerateResponse {
    response: Option<String>,
    #[serde(default)]
    done: bool,
}

#[derive(Debug, Serialize)]
struct OllamaEmbedRequest {
    model: String,
    input: String,
}

#[derive(Debug, Deserialize)]
struct OllamaEmbedResponse {
    embeddings: Option<Vec<Vec<f32>>>,
}

// ─────────────────────────────────────────────
// Mode d'inférence
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum InferenceMode {
    /// Mutations rapides, modèle léger
    Fast,
    /// Analyse profonde, modèle lourd (stagnation)
    Deep,
}

impl std::fmt::Display for InferenceMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InferenceMode::Fast => write!(f, "FAST"),
            InferenceMode::Deep => write!(f, "DEEP"),
        }
    }
}

// ─────────────────────────────────────────────
// Moteur Ollama multi-modèle
// ─────────────────────────────────────────────

pub struct OllamaEngine {
    client: Client,
    base_url: String,
    model_fast: String,
    model_deep: String,
    model_embed: String,
    ctx_fast: usize,
    ctx_deep: usize,
    sem_fast: Arc<Semaphore>,
    sem_deep: Arc<Semaphore>,
    pub current_mode: InferenceMode,
}

impl OllamaEngine {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        base_url: &str,
        model_fast: &str,
        model_deep: &str,
        model_embed: &str,
        ctx_fast: usize,
        ctx_deep: usize,
        max_concurrent_fast: usize,
        max_concurrent_deep: usize,
    ) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.to_string(),
            model_fast: model_fast.to_string(),
            model_deep: model_deep.to_string(),
            model_embed: model_embed.to_string(),
            ctx_fast,
            ctx_deep,
            sem_fast: Arc::new(Semaphore::new(max_concurrent_fast)),
            sem_deep: Arc::new(Semaphore::new(max_concurrent_deep)),
            current_mode: InferenceMode::Fast,
        }
    }

    /// Basculer le mode d'inférence
    pub fn set_mode(&mut self, mode: InferenceMode) {
        if self.current_mode != mode {
            info!("Bascule inférence: {} → {}", self.current_mode, mode);
            self.current_mode = mode;
        }
    }

    /// Vérifie que Ollama est accessible
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/api/tags", self.base_url);
        match self.client.get(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    /// Envoie un prompt au modèle actif
    async fn generate(&self, prompt: &str, temperature: f32) -> Result<String> {
        let (model, ctx, sem) = match self.current_mode {
            InferenceMode::Fast => (&self.model_fast, self.ctx_fast, &self.sem_fast),
            InferenceMode::Deep => (&self.model_deep, self.ctx_deep, &self.sem_deep),
        };

        let _permit = sem.acquire().await?;

        let request = OllamaGenerateRequest {
            model: model.clone(),
            prompt: prompt.to_string(),
            stream: false,
            options: OllamaOptions {
                num_ctx: ctx,
                temperature,
                top_p: 0.9,
            },
        };

        let url = format!("{}/api/generate", self.base_url);
        let response = self.client.post(&url).json(&request).send().await?;
        let resp: OllamaGenerateResponse = response.json().await?;
        Ok(resp.response.unwrap_or_default())
    }

    /// Générer un embedding via le modèle d'embeddings
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let request = OllamaEmbedRequest {
            model: self.model_embed.clone(),
            input: text.chars().take(2000).collect(),
        };

        let url = format!("{}/api/embed", self.base_url);
        let response = self.client.post(&url).json(&request).send().await?;
        let resp: OllamaEmbedResponse = response.json().await?;

        match resp.embeddings {
            Some(vecs) if !vecs.is_empty() => Ok(vecs.into_iter().next().unwrap()),
            _ => Err(anyhow::anyhow!("Pas d'embedding retourné")),
        }
    }

    /// Générer une mutation pour un agent
    /// Le prompt demande du code Rust EXÉCUTABLE
    pub async fn generate_mutation(&self, agent: &Agent, context: &str) -> Result<Mutation> {
        let mutation_type = MutationType::random();

        let prompt = format!(
            r#"Tu es un moteur d'évolution pour agents Rust autonomes.

Agent actuel:
- Rôle: {}
- Fitness: {:.3} (perf={:.3}, stab={:.3})
- Compile: {} | Tourne: {} | Tests: {}/{}
- Génération: {}
- Stagnation: {} cycles

Code actuel:
```rust
{}
```

Type de mutation: {:?}

Contexte web récent:
{}

INSTRUCTIONS CRITIQUES:
- Génère du code Rust COMPILABLE et EXÉCUTABLE
- Le code doit contenir fn main() ou des fonctions pures
- Ajoute des auto-tests : println!("SOULSYSTEM_EVO_TESTS:{{passed}}/{{total}}")
- Le code doit être AUTONOME (pas de dépendances externes)
- Améliore {} par rapport au code existant

Réponds UNIQUEMENT avec le code Rust entre ```rust et ```. Pas d'explication."#,
            agent.role,
            agent.fitness,
            agent.metrics.performance,
            agent.metrics.stability,
            agent.metrics.compiles,
            agent.metrics.runs_ok,
            agent.metrics.tests_passed,
            agent.metrics.tests_total,
            agent.generation,
            agent.stagnation_count,
            if agent.code.is_empty() {
                "// code initial vide"
            } else {
                &agent.code
            },
            mutation_type,
            context.chars().take(800).collect::<String>(),
            mutation_type.label()
        );

        info!(
            "Mutation {:?} pour agent {} [mode={}]",
            mutation_type,
            &agent.id[..8],
            self.current_mode
        );

        match self.generate(&prompt, 0.7).await {
            Ok(response) => {
                let code_patch = self.extract_code_block(&response);
                Ok(Mutation {
                    mutation_type,
                    code_patch,
                    description: response.chars().take(500).collect(),
                })
            }
            Err(e) => {
                warn!("Erreur Ollama: {} — génération code de base", e);
                Ok(Mutation {
                    mutation_type,
                    code_patch: Self::fallback_code(&agent.role),
                    description: format!("Fallback (Ollama: {})", e),
                })
            }
        }
    }

    /// Tâche de curiosité pour exploration
    pub async fn generate_curiosity_task(&self, system_state: &str) -> Result<String> {
        let prompt = format!(
            r#"Tu es un système d'exploration autonome.
État: {}

Génère UNE requête de recherche technique (Rust, IA, systèmes évolutifs).
Réponds en UNE ligne uniquement."#,
            system_state
        );
        self.generate(&prompt, 0.9).await
    }

    /// Analyser du contenu crawlé
    pub async fn analyze_knowledge(&self, content: &str) -> Result<String> {
        let prompt = format!(
            r#"Extrais les connaissances clés de ce contenu pour un système d'agents Rust:

{}

Résume en 3-5 points techniques."#,
            content.chars().take(1500).collect::<String>()
        );
        self.generate(&prompt, 0.3).await
    }

    /// Extraire un bloc de code Rust
    fn extract_code_block(&self, response: &str) -> String {
        // Chercher ```rust ... ```
        if let Some(start) = response.find("```rust") {
            let code_start = start + 7;
            if let Some(end) = response[code_start..].find("```") {
                return response[code_start..code_start + end].trim().to_string();
            }
        }
        // Chercher ``` ... ```
        if let Some(start) = response.find("```") {
            let code_start = start + 3;
            let code_start = response[code_start..]
                .find('\n')
                .map(|n| code_start + n + 1)
                .unwrap_or(code_start);
            if let Some(end) = response[code_start..].find("```") {
                return response[code_start..code_start + end].trim().to_string();
            }
        }
        String::new()
    }

    /// Code de secours quand Ollama est indisponible
    fn fallback_code(role: &crate::agent::Role) -> String {
        match role {
            crate::agent::Role::Explorer => {
                r#"
fn main() {
    let data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let sum: i64 = data.iter().sum();
    let mean = sum as f64 / data.len() as f64;
    let passed = if (mean - 5.5).abs() < 0.01 { 1 } else { 0 };
    println!("SOULSYSTEM_EVO_TESTS:{}/1", passed);
    println!("SOULSYSTEM_EVO_STATUS:OK");
}"#
            }
            crate::agent::Role::Optimizer => {
                r#"
fn main() {
    let mut v: Vec<i32> = (0..1000).rev().collect();
    v.sort_unstable();
    let sorted = v.windows(2).all(|w| w[0] <= w[1]);
    let passed = if sorted { 1 } else { 0 };
    println!("SOULSYSTEM_EVO_TESTS:{}/1", passed);
    println!("SOULSYSTEM_EVO_STATUS:OK");
}"#
            }
            _ => {
                r#"
fn main() {
    let result = (0..100).filter(|x| x % 2 == 0).count();
    let passed = if result == 50 { 1 } else { 0 };
    println!("SOULSYSTEM_EVO_TESTS:{}/1", passed);
    println!("SOULSYSTEM_EVO_STATUS:OK");
}"#
            }
        }
        .trim()
        .to_string()
    }
}
