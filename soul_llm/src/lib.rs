//! # soul_llm — Client HTTP pur Rust vers Ollama
//! Le cerveau de SoulSystem. Aucune dépendance Python.

use serde::{Deserialize, Serialize};
use std::time::Duration;

// ── Config ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub base_url: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: usize,
    /// Timeout global pour les requêtes HTTP. Défaut : 30s.
    pub http_timeout: Duration,
    /// Timeout de connect uniquement. Défaut : 5s.
    pub connect_timeout: Duration,
    /// Authentification optionnelle (token Bearer).
    pub auth_token: Option<String>,
    /// Budget maximal de tokens par but (goal). 0 = illimité.
    pub goal_token_budget: usize,
    /// Budget maximal de tokens par minute. 0 = illimité.
    pub tokens_per_minute_budget: usize,
    /// Taille maximale du pool de connexions HTTP. Défaut : 10.
    pub pool_max_idle: usize,
    /// Keep-alive timeout pour les connexions inactives. Défaut : 30s.
    pub pool_idle_timeout: Duration,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:11434".into(),
            model: "qwen3:8b".into(),
            temperature: 0.7,
            max_tokens: 2048,
            http_timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(5),
            auth_token: None,
            goal_token_budget: 50000,
            tokens_per_minute_budget: 100000,
            pool_max_idle: 10,
            pool_idle_timeout: Duration::from_secs(30),
        }
    }
}

/// Statistiques d'utilisation LLM pour un goal
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
    pub total_duration_ms: u64,
    pub request_count: usize,
}

impl LlmUsage {
    pub fn add(&mut self, resp: &GenerateResponse) {
        self.prompt_tokens += resp.prompt_eval_count;
        self.completion_tokens += resp.eval_count;
        self.total_tokens += resp.prompt_eval_count + resp.eval_count;
        self.total_duration_ms += resp.total_duration / 1_000_000;
        self.request_count += 1;
    }
}

/// Budget LLM avec suivi de consommation
#[derive(Debug)]
pub struct LlmBudget {
    config: LlmConfig,
    goal_usage: dashmap::DashMap<String, LlmUsage>,
    minute_usage: dashmap::DashMap<String, LlmUsage>, // key: "YYYY-MM-DD-HH-MM"
    minute_reset: parking_lot::Mutex<chrono::DateTime<chrono::Utc>>,
}

impl LlmBudget {
    pub fn new(config: LlmConfig) -> Self {
        Self {
            config,
            goal_usage: dashmap::DashMap::new(),
            minute_usage: dashmap::DashMap::new(),
            minute_reset: parking_lot::Mutex::new(chrono::Utc::now()),
        }
    }

    fn current_minute_key(&self) -> String {
        chrono::Utc::now().format("%Y-%m-%d-%H-%M").to_string()
    }

    fn maybe_reset_minute(&self) {
        let mut reset = self.minute_reset.lock();
        let now = chrono::Utc::now();
        if now.signed_duration_since(*reset).num_minutes() >= 1 {
            self.minute_usage.clear();
            *reset = now;
        }
    }

    /// Vérifie si la requête peut être faite sans dépasser les budgets
    pub fn check_budget(&self, goal_id: &str, estimated_tokens: usize) -> Result<(), String> {
        if self.config.goal_token_budget > 0 {
            if let Some(usage) = self.goal_usage.get(goal_id) {
                if usage.total_tokens + estimated_tokens > self.config.goal_token_budget {
                    return Err(format!("Goal {} exceeded token budget: {} > {}", 
                        goal_id, usage.total_tokens + estimated_tokens, self.config.goal_token_budget));
                }
            }
        }

        if self.config.tokens_per_minute_budget > 0 {
            self.maybe_reset_minute();
            let minute_key = self.current_minute_key();
            if let Some(usage) = self.minute_usage.get(&minute_key) {
                if usage.total_tokens + estimated_tokens > self.config.tokens_per_minute_budget {
                    return Err(format!("Minute token budget exceeded: {} > {}", 
                        usage.total_tokens + estimated_tokens, self.config.tokens_per_minute_budget));
                }
            }
        }
        Ok(())
    }

    /// Enregistre l'usage après une requête
    pub fn record_usage(&self, goal_id: &str, resp: &GenerateResponse) {
        let _tokens = resp.prompt_eval_count + resp.eval_count;
        
        self.goal_usage
            .entry(goal_id.to_string())
            .or_default()
            .add(resp);

        if self.config.tokens_per_minute_budget > 0 {
            let minute_key = self.current_minute_key();
            self.minute_usage
                .entry(minute_key)
                .or_default()
                .add(resp);
        }
    }

    /// Obtient l'usage pour un goal
    pub fn get_goal_usage(&self, goal_id: &str) -> Option<LlmUsage> {
        self.goal_usage.get(goal_id).map(|u| u.clone())
    }

    /// Obtient l'usage de la minute courante
    pub fn get_minute_usage(&self) -> LlmUsage {
        let minute_key = self.current_minute_key();
        self.minute_usage.get(&minute_key).map(|u| u.clone()).unwrap_or_default()
    }

    /// Reset l'usage d'un goal (ex: après completion)
    pub fn reset_goal(&self, goal_id: &str) {
        self.goal_usage.remove(goal_id);
    }

    /// Retourne tous les usages par goal
    pub fn all_goal_usages(&self) -> Vec<(String, LlmUsage)> {
        self.goal_usage.iter().map(|r| (r.key().clone(), r.value().clone())).collect()
    }
}

// ── Types Ollama ─────────────────────────────────────────────

#[derive(Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    prompt: String,
    stream: bool,
}

#[derive(Deserialize)]
pub struct GenerateResponse {
    pub response: String,
    #[allow(dead_code)]
    pub done: bool,
    #[serde(default)]
    pub prompt_eval_count: usize,
    #[serde(default)]
    pub eval_count: usize,
    #[serde(default)]
    pub total_duration: u64,
    #[serde(default)]
    pub load_duration: u64,
    #[serde(default)]
    pub prompt_eval_duration: u64,
    #[serde(default)]
    pub eval_duration: u64,
}

#[derive(Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    input: String,
}

#[derive(Deserialize)]
pub struct EmbedResponse {
    pub embedding: Vec<f32>,
}

// ── Client ───────────────────────────────────────────────────

pub struct OllamaClient {
    config: LlmConfig,
    http: reqwest::Client,
    budget: std::sync::Arc<LlmBudget>,
}

impl OllamaClient {
    pub fn new(config: LlmConfig) -> Self {
        let mut builder = reqwest::Client::builder()
            .timeout(config.http_timeout)
            .connect_timeout(config.connect_timeout)
            .pool_max_idle_per_host(config.pool_max_idle)
            .pool_idle_timeout(config.pool_idle_timeout);
        if let Some(ref token) = config.auth_token {
            let mut headers = reqwest::header::HeaderMap::new();
            if let Ok(v) = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}")) {
                headers.insert(reqwest::header::AUTHORIZATION, v);
            }
            builder = builder.default_headers(headers);
        }
        let budget = std::sync::Arc::new(LlmBudget::new(config.clone()));
        Self {
            config,
            http: builder.build().unwrap_or_else(|_| reqwest::Client::new()),
            budget,
        }
    }

    pub fn budget(&self) -> &LlmBudget {
        &self.budget
    }

    pub async fn is_alive(&self) -> bool {
        self.http
            .get(format!("{}/api/tags", self.config.base_url))
            .send()
            .await
            .is_ok()
    }

    pub async fn list_models(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let resp = self
            .http
            .get(format!("{}/api/tags", self.config.base_url))
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        let models: Vec<String> = if let Some(arr) = resp["models"].as_array() {
            arr.iter()
                .filter_map(|m| m["name"].as_str().map(String::from))
                .collect()
        } else {
            vec![]
        };

        Ok(models)
    }

    pub async fn generate(
        &self,
        prompt: &str,
    ) -> Result<GenerateResponse, Box<dyn std::error::Error>> {
        self.generate_with_goal(prompt, "default").await
    }

    /// Génère avec suivi de budget pour un goal spécifique
    pub async fn generate_with_goal(
        &self,
        prompt: &str,
        goal_id: &str,
    ) -> Result<GenerateResponse, Box<dyn std::error::Error>> {
        // Estimation grossière: 1 token ≈ 4 caractères
        let estimated = prompt.len() / 4 + self.config.max_tokens;
        self.budget.check_budget(goal_id, estimated)?;

        let req = GenerateRequest {
            model: &self.config.model,
            prompt: prompt.into(),
            stream: false,
        };

        let url = format!("{}/api/generate", self.config.base_url);
        let resp: GenerateResponse = self.http.post(&url).json(&req).send().await?.json().await?;

        self.budget.record_usage(goal_id, &resp);

        Ok(resp)
    }

    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        let req = EmbedRequest {
            model: &self.config.model,
            input: text.into(),
        };

        let url = format!("{}/api/embed", self.config.base_url);
        let resp: EmbedResponse = self.http.post(&url).json(&req).send().await?.json().await?;

        Ok(resp.embedding)
    }

    pub async fn embed_batch(
        &self,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>, Box<dyn std::error::Error>> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }
}
