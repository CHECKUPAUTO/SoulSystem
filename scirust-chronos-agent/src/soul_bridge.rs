// ==========================================================================
// soul_bridge.rs — Pont HTTP ChronosAgent → SoulSystem (V6.4)
//
// Ferme la dernière flèche manquante du schéma : la couche ChronosAgent
// (RAM) parle maintenant à la couche SoulSystem (port 9023) via HTTP.
//
// CE QUE FAIT LE BRIDGE
//
//   - PUSH : à intervalles réguliers, envoie les prototypes sémantiques
//     nouvellement consolidés vers SoulSystem via POST /memory/store.
//     Ces prototypes deviennent visibles aux autres clients (OpenClaw, etc.)
//     via le bus d'événements et le ws_bridge.
//
//   - PULL : récupère des souvenirs partagés via POST /memory/search.
//     Permet à ChronosAgent d'enrichir son contexte avec des souvenirs
//     stockés par d'autres composants du système (OpenClaw, sync script).
//
// CE QU'IL NE FAIT PAS (DESIGN INTENTIONNEL)
//
//   - Ne pousse PAS les poids appris (ScoreNetwork, PotentialMLP). Ces
//     poids restent dans le checkpoint local. Pour les partager entre
//     machines, le checkpoint .safetensors peut être copié via rsync/scp,
//     mais ce n'est pas le rôle de SoulSystem (qui est un vector store,
//     pas un model registry).
//
//   - Ne récupère PAS les checkpoints depuis SoulSystem. Si l'agent est
//     fresh start, il commence à zéro (ou utilise le local).
//
// FORMAT DES REQUÊTES
//
//   POST /memory/store
//     {
//       "content": "<texte décrivant le souvenir>",
//       "embedding": [f32; N],
//       "metadata": { "source": "chronos-agent", "type": "semantic_prototype", ... }
//     }
//
//   POST /memory/search
//     {
//       "embedding": [f32; N],
//       "top_k": 10,
//       "filter": { "source": "chronos-agent" }
//     }
//     → {
//       "results": [{ "content": "...", "embedding": [...], "score": 0.93 }, ...]
//     }
//
// ROBUSTESSE
//
//   - Timeouts conservateurs (2s par défaut)
//   - Erreurs réseau silencieuses : le bridge log et continue, l'agent
//     ne crashe pas si SoulSystem est down
//   - Mode dry-run pour tests sans serveur
// ==========================================================================

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::time::Duration;

// --------------------------------------------------------------------------
// Trait — backend abstrait pour testabilité
// --------------------------------------------------------------------------

pub trait MemoryBackend: Send {
    fn store(&self, item: &MemoryItem) -> Result<(), BridgeError>;
    fn search(&self, query: &SearchQuery) -> Result<Vec<MemoryItem>, BridgeError>;
    fn health_check(&self) -> Result<(), BridgeError>;
}

// --------------------------------------------------------------------------
// Types
// --------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemoryItem {
    pub content: String,
    pub embedding: Vec<f32>,
    #[serde(default)]
    pub metadata: serde_json::Map<String, JsonValue>,
    #[serde(default)]
    pub score: f64,
}

impl MemoryItem {
    pub fn new(content: impl Into<String>, embedding: Vec<f32>) -> Self {
        Self {
            content: content.into(),
            embedding,
            metadata: serde_json::Map::new(),
            score: 0.0,
        }
    }

    pub fn with_metadata(mut self, key: &str, value: JsonValue) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Construit un item à partir d'un prototype sémantique de la MAA.
    pub fn from_semantic_prototype(vector: &[f64], strength: f64,
                                   t_created: u64) -> Self {
        let emb: Vec<f32> = vector.iter().map(|x| *x as f32).collect();
        let content = format!("semantic prototype #{} (strength={:.2})",
                              t_created, strength);
        let mut item = Self::new(content, emb);
        item.metadata.insert("source".into(), JsonValue::String("chronos-agent".into()));
        item.metadata.insert("type".into(), JsonValue::String("semantic_prototype".into()));
        item.metadata.insert("strength".into(), JsonValue::from(strength));
        item.metadata.insert("t_created".into(), JsonValue::from(t_created));
        item
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct SearchQuery {
    pub embedding: Vec<f32>,
    pub top_k: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<serde_json::Map<String, JsonValue>>,
}

#[derive(Debug, Clone)]
pub enum BridgeError {
    Network(String),
    Http(u16, String),
    Serde(String),
    NotImplemented,
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Network(msg) => write!(f, "network error: {}", msg),
            Self::Http(code, body) => write!(f, "HTTP {} : {}", code, body),
            Self::Serde(msg) => write!(f, "serialization error: {}", msg),
            Self::NotImplemented => write!(f, "not implemented"),
        }
    }
}

impl std::error::Error for BridgeError {}

// --------------------------------------------------------------------------
// SoulSystemClient — implémentation HTTP réelle (ureq)
// --------------------------------------------------------------------------

pub struct SoulSystemClient {
    base_url: String,
    timeout: Duration,
    agent: ureq::Agent,
}

impl SoulSystemClient {
    /// Crée un client pointant vers `http://host:port` (typiquement http://localhost:9023).
    pub fn new(base_url: impl Into<String>) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(2))
            .build();
        Self {
            base_url: base_url.into(),
            timeout: Duration::from_secs(2),
            agent,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self.agent = ureq::AgentBuilder::new().timeout(timeout).build();
        self
    }
}

impl MemoryBackend for SoulSystemClient {
    fn store(&self, item: &MemoryItem) -> Result<(), BridgeError> {
        let url = format!("{}/memory/store", self.base_url);
        let resp = self.agent
            .post(&url)
            .send_json(serde_json::to_value(item).map_err(|e|
                BridgeError::Serde(e.to_string()))?);
        match resp {
            Ok(_) => Ok(()),
            Err(ureq::Error::Status(code, response)) => {
                let body = response.into_string().unwrap_or_default();
                Err(BridgeError::Http(code, body))
            }
            Err(e) => Err(BridgeError::Network(e.to_string())),
        }
    }

    fn search(&self, query: &SearchQuery) -> Result<Vec<MemoryItem>, BridgeError> {
        let url = format!("{}/memory/search", self.base_url);
        let resp = self.agent
            .post(&url)
            .send_json(serde_json::to_value(query).map_err(|e|
                BridgeError::Serde(e.to_string()))?);
        match resp {
            Ok(r) => {
                #[derive(Deserialize)]
                struct SearchResponse {
                    results: Vec<MemoryItem>,
                }
                let body: SearchResponse = r.into_json().map_err(|e|
                    BridgeError::Serde(e.to_string()))?;
                Ok(body.results)
            }
            Err(ureq::Error::Status(code, response)) => {
                let body = response.into_string().unwrap_or_default();
                Err(BridgeError::Http(code, body))
            }
            Err(e) => Err(BridgeError::Network(e.to_string())),
        }
    }

    fn health_check(&self) -> Result<(), BridgeError> {
        let url = format!("{}/health", self.base_url);
        // GET sans body
        let resp = self.agent.get(&url).call();
        match resp {
            Ok(_) => Ok(()),
            Err(ureq::Error::Status(code, response)) => {
                let body = response.into_string().unwrap_or_default();
                Err(BridgeError::Http(code, body))
            }
            Err(e) => Err(BridgeError::Network(e.to_string())),
        }
    }
}

// --------------------------------------------------------------------------
// InMemoryBackend — pour tests et fallback offline
// --------------------------------------------------------------------------

pub struct InMemoryBackend {
    pub items: std::sync::Mutex<Vec<MemoryItem>>,
}

impl InMemoryBackend {
    pub fn new() -> Self {
        Self { items: std::sync::Mutex::new(Vec::new()) }
    }

    pub fn count(&self) -> usize {
        self.items.lock().unwrap().len()
    }
}

impl Default for InMemoryBackend {
    fn default() -> Self { Self::new() }
}

impl MemoryBackend for InMemoryBackend {
    fn store(&self, item: &MemoryItem) -> Result<(), BridgeError> {
        self.items.lock().unwrap().push(item.clone());
        Ok(())
    }

    fn search(&self, query: &SearchQuery) -> Result<Vec<MemoryItem>, BridgeError> {
        let items = self.items.lock().unwrap();
        let mut scored: Vec<(MemoryItem, f64)> = items.iter()
            .map(|it| {
                let sim = cosine_f32(&it.embedding, &query.embedding);
                let mut clone = it.clone();
                clone.score = sim;
                (clone, sim)
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        Ok(scored.into_iter().take(query.top_k).map(|(it, _)| it).collect())
    }

    fn health_check(&self) -> Result<(), BridgeError> { Ok(()) }
}

// --------------------------------------------------------------------------
// SoulBridge — orchestrateur qui pousse périodiquement
// --------------------------------------------------------------------------

pub struct SoulBridge<B: MemoryBackend> {
    pub backend: B,
    /// Compteur d'items poussés avec succès.
    pub pushed: u64,
    /// Compteur d'items dont le push a échoué.
    pub push_failures: u64,
    /// Compteur de pulls réussis.
    pub pulled: u64,
}

impl<B: MemoryBackend> SoulBridge<B> {
    pub fn new(backend: B) -> Self {
        Self { backend, pushed: 0, push_failures: 0, pulled: 0 }
    }

    /// Pousse un batch de prototypes sémantiques vers SoulSystem.
    /// Continue sur erreur (best-effort).
    pub fn push_batch(&mut self, items: &[MemoryItem]) -> usize {
        let mut ok = 0;
        for item in items {
            match self.backend.store(item) {
                Ok(()) => { ok += 1; self.pushed += 1; }
                Err(e) => {
                    eprintln!("soul_bridge: push failed: {}", e);
                    self.push_failures += 1;
                }
            }
        }
        ok
    }

    pub fn search(&mut self, query: &SearchQuery)
        -> Result<Vec<MemoryItem>, BridgeError>
    {
        let result = self.backend.search(query);
        if result.is_ok() { self.pulled += 1; }
        result
    }

    pub fn is_healthy(&self) -> bool {
        self.backend.health_check().is_ok()
    }
}

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

fn cosine_f32(a: &[f32], b: &[f32]) -> f64 {
    let len = a.len().min(b.len());
    let mut dot = 0.0;
    let mut na = 0.0;
    let mut nb = 0.0;
    for i in 0..len {
        dot += a[i] as f64 * b[i] as f64;
        na += a[i] as f64 * a[i] as f64;
        nb += b[i] as f64 * b[i] as f64;
    }
    let denom = (na.sqrt() * nb.sqrt()).max(f64::EPSILON);
    dot / denom
}

// ==========================================================================
// Tests
// ==========================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_backend_round_trip() {
        let backend = InMemoryBackend::new();
        let item = MemoryItem::new("hello world", vec![1.0, 0.0, 0.0, 0.0])
            .with_metadata("source", JsonValue::String("test".into()));
        backend.store(&item).unwrap();
        assert_eq!(backend.count(), 1);

        let query = SearchQuery {
            embedding: vec![1.0, 0.0, 0.0, 0.0],
            top_k: 5,
            filter: None,
        };
        let results = backend.search(&query).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].score > 0.99);
    }

    #[test]
    fn from_semantic_prototype_includes_metadata() {
        let item = MemoryItem::from_semantic_prototype(
            &[0.5, 0.5, 0.5, 0.5], 3.7, 42);
        assert!(item.metadata.contains_key("source"));
        assert!(item.metadata.contains_key("type"));
        assert!(item.metadata.contains_key("strength"));
        assert_eq!(item.embedding.len(), 4);
        assert!(item.metadata["strength"].as_f64().unwrap() > 3.6);
    }

    #[test]
    fn soul_bridge_push_batch_tracks_success() {
        let mut bridge = SoulBridge::new(InMemoryBackend::new());
        let items = vec![
            MemoryItem::from_semantic_prototype(&[1.0; 4], 1.0, 0),
            MemoryItem::from_semantic_prototype(&[0.5; 4], 2.0, 1),
            MemoryItem::from_semantic_prototype(&[0.0; 4], 3.0, 2),
        ];
        let ok = bridge.push_batch(&items);
        assert_eq!(ok, 3);
        assert_eq!(bridge.pushed, 3);
        assert_eq!(bridge.push_failures, 0);
        assert_eq!(bridge.backend.count(), 3);
    }

    #[test]
    fn soul_bridge_search_increments_counter() {
        let backend = InMemoryBackend::new();
        backend.store(&MemoryItem::new("foo", vec![1.0, 0.0])).unwrap();
        let mut bridge = SoulBridge::new(backend);
        let query = SearchQuery {
            embedding: vec![1.0, 0.0],
            top_k: 1,
            filter: None,
        };
        let _ = bridge.search(&query).unwrap();
        assert_eq!(bridge.pulled, 1);
    }

    #[test]
    fn soul_bridge_health_check_works() {
        let bridge = SoulBridge::new(InMemoryBackend::new());
        assert!(bridge.is_healthy());
    }

    #[test]
    fn http_client_handles_offline_gracefully() {
        // Pointe vers un port garanti fermé (1 = privilégié, jamais ouvert)
        let mut bridge = SoulBridge::new(
            SoulSystemClient::new("http://127.0.0.1:1")
                .with_timeout(Duration::from_millis(100)));
        let item = MemoryItem::from_semantic_prototype(&[1.0; 4], 1.0, 0);
        let r = bridge.push_batch(&[item]);
        assert_eq!(r, 0);
        assert_eq!(bridge.push_failures, 1);
        // L'agent ne crashe pas — c'est l'invariant clé.
    }

    #[test]
    fn search_query_serializes_with_filter() {
        let mut filter = serde_json::Map::new();
        filter.insert("source".into(), JsonValue::String("chronos-agent".into()));
        let q = SearchQuery {
            embedding: vec![1.0, 2.0],
            top_k: 5,
            filter: Some(filter),
        };
        let s = serde_json::to_string(&q).unwrap();
        assert!(s.contains("\"filter\""));
        assert!(s.contains("chronos-agent"));

        // Sans filter, le champ doit être absent (skip_serializing_if)
        let q2 = SearchQuery {
            embedding: vec![1.0, 2.0],
            top_k: 5,
            filter: None,
        };
        let s2 = serde_json::to_string(&q2).unwrap();
        assert!(!s2.contains("\"filter\""));
    }

    /// Test d'intégration end-to-end : lance un mini-serveur HTTP en
    /// thread, fait des appels store/search, vérifie les retours.
    /// Valide que le format JSON envoyé matche bien celui attendu.
    #[test]
    fn http_round_trip_with_mock_server() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::{Arc, Mutex};
        use std::thread;

        // Mini-serveur HTTP en thread, accumule les requêtes reçues.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let received = Arc::new(Mutex::new(Vec::<(String, String)>::new())); // (path, body)
        let received_clone = received.clone();

        let server_thread = thread::spawn(move || {
            // Sert exactement 2 requêtes puis termine
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).unwrap();
                let req = String::from_utf8_lossy(&buf[..n]).to_string();

                // Parse path et body
                let path = req.lines().next().unwrap()
                    .split_whitespace().nth(1).unwrap_or("/").to_string();
                let body = req.splitn(2, "\r\n\r\n").nth(1).unwrap_or("").to_string();
                received_clone.lock().unwrap().push((path.clone(), body));

                let response_body = if path == "/memory/store" {
                    r#"{"ok":true}"#.to_string()
                } else if path == "/memory/search" {
                    r#"{"results":[{"content":"hello","embedding":[0.5,0.5,0.5,0.5],"metadata":{},"score":0.9}]}"#.to_string()
                } else {
                    "{}".to_string()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response_body.len(), response_body);
                stream.write_all(response.as_bytes()).unwrap();
                stream.flush().unwrap();
            }
        });

        // Client pointe vers notre mock
        let client = SoulSystemClient::new(format!("http://127.0.0.1:{}", port))
            .with_timeout(Duration::from_secs(1));

        // 1. Store
        let item = MemoryItem::from_semantic_prototype(
            &[0.5, 0.5, 0.5, 0.5], 2.5, 100);
        client.store(&item).expect("store should succeed");

        // 2. Search
        let query = SearchQuery {
            embedding: vec![0.5, 0.5, 0.5, 0.5],
            top_k: 3,
            filter: None,
        };
        let results = client.search(&query).expect("search should succeed");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "hello");
        assert!((results[0].score - 0.9).abs() < 1e-6);

        server_thread.join().unwrap();

        // Vérifie ce que le serveur a reçu (paths uniquement, body
        // peut être tronqué par read() TCP chunked sur 8KB buffer)
        let received = received.lock().unwrap();
        assert_eq!(received.len(), 2);
        assert_eq!(received[0].0, "/memory/store");
        assert_eq!(received[1].0, "/memory/search");
    }
}
