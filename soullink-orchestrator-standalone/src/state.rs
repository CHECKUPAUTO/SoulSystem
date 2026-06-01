//! SoulLink Orchestrator - Application State

use crate::models::types::BrainConfig;
use dashmap::DashMap;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::task::JoinSet;
use tracing::info;

/// État global de l'application
#[derive(Clone)]
pub struct AppState {
    pub brains: Arc<DashMap<String, BrainConfig>>,
    pub brain_dir: String,
    query_count: Arc<AtomicU64>,
    spawn_count: Arc<AtomicU64>,
}

impl AppState {
    pub fn new(brain_dir: &str) -> Self {
        let default_brains = Self::create_default_brains();
        let brains: DashMap<String, BrainConfig> = DashMap::new();
        
        for (k, v) in default_brains {
            brains.insert(k, v);
        }

        Self {
            brains: Arc::new(brains),
            brain_dir: brain_dir.to_string(),
            query_count: Arc::new(AtomicU64::new(0)),
            spawn_count: Arc::new(AtomicU64::new(0)),
        }
    }

    fn create_default_brains() -> HashMap<String, BrainConfig> {
        let mut brains = HashMap::new();

        let specs = [
            ("science", 9010u16, vec!["physics", "math", "chemistry", "computation", "science"]),
            ("mind", 9011, vec!["neuroscience", "language", "philosophy", "memory", "mind"]),
            ("engineer", 9012, vec!["optimization", "logic", "algebra", "computation", "engineering"]),
            ("crypto", 9013, vec!["trading", "blockchain", "defi", "markets", "crypto", "finance"]),
            ("creative", 9014, vec!["patterns", "geometry", "art", "vision", "design", "creative"]),
            ("meta", 9015, vec!["learning", "optimization", "meta", "reinforcement"]),
        ];

        for (name, port, spec) in &specs {
            brains.insert(
                name.to_string(),
                BrainConfig {
                    port: *port,
                    url: format!("http://localhost:{}", port),
                    speciality: spec.iter().map(|s| s.to_string()).collect(),
                    version: "v12".to_string(),
                    spawned_by: "default".to_string(),
                },
            );
        }

        brains
    }

    /// Appelle un cerveau spécifique
    pub async fn call_brain(
        &self,
        url: &str,
        endpoint: &str,
        body: Option<Value>,
    ) -> Result<Value, String> {
        let full_url = format!("{}{}", url, endpoint);

        tokio::task::spawn_blocking(move || {
            let result = if let Some(b) = body {
                let body_str = serde_json::to_string(&b).map_err(|e| e.to_string())?;
                minreq::post(&full_url)
                    .with_header("Content-Type", "application/json")
                    .with_body(body_str)
                    .with_timeout(4)
                    .send()
                    .map_err(|e| e.to_string())
            } else {
                minreq::get(&full_url)
                    .with_timeout(4)
                    .send()
                    .map_err(|e| e.to_string())
            };

            match result {
                Ok(resp) => {
                    let body_str = resp.as_str().map_err(|e| e.to_string())?;
                    serde_json::from_str::<Value>(body_str).map_err(|e| e.to_string())
                }
                Err(e) => Err(e),
            }
        })
        .await
        .map_err(|e| e.to_string())?
    }

    /// Appelle tous les cerveaux en parallèle
    pub async fn call_all_parallel(
        &self,
        endpoint: &str,
        body: Option<Value>,
        keys: Option<Vec<String>>,
    ) -> HashMap<String, Value> {
        let keys = keys.unwrap_or_else(|| {
            self.brains.iter().map(|e| e.key().clone()).collect()
        });

        let mut results = HashMap::new();
        let mut set = JoinSet::new();

        for key in keys {
            let url = self.brains.get(&key).map(|b| b.url.clone()).unwrap_or_default();
            let ep = endpoint.to_string();
            let body = body.clone();

            set.spawn(async move {
                let result = Self::call_brain_static(&url, &ep, body).await;
                (key, result.unwrap_or_else(|e| json!({"error": e})))
            });
        }

        while let Some(Ok((key, value))) = set.join_next().await {
            results.insert(key, value);
        }

        results
    }

    async fn call_brain_static(
        url: &str,
        endpoint: &str,
        body: Option<Value>,
    ) -> Result<Value, String> {
        let full_url = format!("{}{}", url, endpoint);
        let full_url_clone = full_url.clone();

        tokio::task::spawn_blocking(move || {
            let result = if let Some(b) = body {
                let body_str = serde_json::to_string(&b).map_err(|e| e.to_string())?;
                minreq::post(&full_url_clone)
                    .with_header("Content-Type", "application/json")
                    .with_body(body_str)
                    .with_timeout(4)
                    .send()
                    .map_err(|e| e.to_string())
            } else {
                minreq::get(&full_url_clone)
                    .with_timeout(4)
                    .send()
                    .map_err(|e| e.to_string())
            };

            match result {
                Ok(resp) => {
                    let body_str = resp.as_str().map_err(|e| e.to_string())?;
                    serde_json::from_str::<Value>(body_str).map_err(|e| e.to_string())
                }
                Err(e) => Err(e),
            }
        })
        .await
        .map_err(|e| e.to_string())?
    }

    /// Sélectionne les cerveaux les plus pertinents pour une requête
    pub fn select_brains(&self, query: &str) -> Vec<String> {
        let q = query.to_lowercase();
        
        let mut scores: Vec<(String, f64)> = self
            .brains
            .iter()
            .map(|entry| {
                let key = entry.key().clone();
                let score: f64 = entry
                    .value()
                    .speciality
                    .iter()
                    .map(|kw| if q.contains(kw.as_str()) { 2.0 } else { 0.0 })
                    .sum();
                let bonus = if key == "meta" { 0.5 } else { 0.0 };
                (key, score + bonus)
            })
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        let mut selected: Vec<String> = scores
            .iter()
            .take(3)
            .map(|(k, _)| k.clone())
            .collect();
        
        if !selected.contains(&"meta".to_string()) {
            selected.push("meta".to_string());
        }
        selected.dedup();
        selected
    }

    /// Incrémente le compteur de requêtes
    pub fn increment_query_count(&self) {
        self.query_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Incrémente le compteur de spawns
    pub fn increment_spawn_count(&self) {
        self.spawn_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Récupère le nombre de requêtes
    pub fn get_query_count(&self) -> u64 {
        self.query_count.load(Ordering::Relaxed)
    }

    /// Récupère le nombre de spawns
    pub fn get_spawn_count(&self) -> u64 {
        self.spawn_count.load(Ordering::Relaxed)
    }

    /// Nombre de cerveaux
    pub fn brains_count(&self) -> usize {
        self.brains.len()
    }

    /// Vérifie si un cerveau existe
    pub fn brain_exists(&self, name: &str) -> bool {
        self.brains.contains_key(name)
    }

    /// Récupère le port d'un cerveau
    pub fn get_brain_port(&self, name: &str) -> Option<u16> {
        self.brains.get(name).map(|b| b.port)
    }

    /// Liste des ports utilisés
    pub fn get_used_ports(&self) -> Vec<u16> {
        self.brains.iter().map(|e| e.value().port).collect()
    }

    /// Itérateur sur les cerveaux
    pub fn brains_iter(&self) -> dashmap::iter::Iter<String, BrainConfig> {
        self.brains.iter()
    }
}
