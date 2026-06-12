use anyhow::Result;
use soul_core::{Goal, GoalStatus, Outcome};
use soul_embed::Embedder;
use soul_memory_store::SharedVectorStore;
use std::collections::HashMap;
use std::sync::Arc;

pub struct SelfModel {
    beliefs: HashMap<String, f64>,
}

impl SelfModel {
    pub fn new() -> Self {
        Self {
            beliefs: HashMap::new(),
        }
    }

    pub fn update(&mut self, capability: &str, outcome: &Outcome) {
        let delta = match outcome {
            Outcome::Success(val) => *val * 0.1,
            Outcome::Failure(val) => -(*val * 0.1),
        };
        let entry = self.beliefs.entry(capability.to_string()).or_insert(0.0);
        *entry = (*entry + delta).clamp(-1.0, 1.0);
    }

    pub fn confidence(&self, capability: &str) -> f64 {
        *self.beliefs.get(capability).unwrap_or(&0.0)
    }
}

pub struct NarrativeMemory {
    store: Arc<SharedVectorStore>,
}

impl NarrativeMemory {
    pub fn new(store: Arc<SharedVectorStore>) -> Self {
        Self { store }
    }

    pub fn add_episode(&self, text: &str, embedding: &[f32]) -> usize {
        // Compute a fast hash for dedup
        let hash = seahash::hash(text.as_bytes());
        // Store as raw bytes
        let metadata = bincode::serialize(&(hash, text.to_string()))
            .unwrap_or_else(|_| text.as_bytes().to_vec());
        self.store.insert(embedding.to_vec(), metadata)
    }

    pub fn search_by_embedding(&self, query: &[f32], k: usize) -> Vec<(f32, String)> {
        self.store
            .search(query, k)
            .into_iter()
            .filter_map(|(dist, id)| {
                if let Some(meta) = self.store.get_metadata(id) {
                    bincode::deserialize::<(u64, String)>(&meta)
                        .ok()
                        .map(|(_, text)| (dist, text))
                } else {
                    None
                }
            })
            .collect()
    }
}

pub struct Identity {
    narrative_memory: Arc<NarrativeMemory>,
    embedder: Embedder,
    goals: Vec<Goal>,
    self_model: SelfModel,
}

impl Identity {
    pub fn new(embedder: Embedder, memory_store: Arc<SharedVectorStore>) -> Self {
        Self {
            narrative_memory: Arc::new(NarrativeMemory::new(memory_store)),
            embedder,
            goals: Vec::new(),
            self_model: SelfModel::new(),
        }
    }

    pub async fn record_episode(
        &mut self,
        subject: &str,
        predicate: &str,
        object: &str,
        _salience: f64,
        outcome: Option<Outcome>,
    ) -> Result<()> {
        let text = format!("{} {} {}", subject, predicate, object);
        let embedding = self.embedder.embed(&text).await?;
        let emb_f32: Vec<f32> = embedding.iter().map(|&x| x as f32).collect();
        self.narrative_memory.add_episode(&text, &emb_f32);
        if subject == "self" {
            if let Some(ref o) = outcome {
                self.self_model.update(predicate, o);
            }
        }
        Ok(())
    }

    pub async fn recall(&self, query: &str, k: usize) -> Result<Vec<(f32, String)>> {
        let embedding = self.embedder.embed(query).await?;
        let emb_f32: Vec<f32> = embedding.iter().map(|&x| x as f32).collect();
        Ok(self.narrative_memory.search_by_embedding(&emb_f32, k))
    }

    pub fn add_goal(&mut self, description: &str, embedding: Vec<f64>, deadline: Option<u64>) {
        let goal = Goal {
            id: uuid::Uuid::new_v4().to_string(),
            description: description.to_string(),
            embedding,
            priority: 0.5,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            deadline,
            status: GoalStatus::Active,
        };
        self.goals.push(goal);
    }

    pub fn top_goal(&self) -> Option<&Goal> {
        self.goals
            .iter()
            .filter(|g| g.status == GoalStatus::Active)
            .max_by(|a, b| a.priority.partial_cmp(&b.priority).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soul_embed::Embedder;
    use soul_memory_store::SharedVectorStore;

    #[tokio::test]
    async fn test_record_episode() {
        let embedder = Embedder::new(&["test"]);
        let store = SharedVectorStore::new(128, 10);
        let mut id = Identity::new(embedder, Arc::new(store));
        let result = id
            .record_episode("self", "feels", "happy", 0.5, Some(Outcome::Success(1.0)))
            .await;
        assert!(result.is_ok(), "Recording episode should succeed");
    }

    #[tokio::test]
    async fn test_add_goal_and_top() {
        let embedder = Embedder::new(&["test"]);
        let store = SharedVectorStore::new(128, 10);
        let mut id = Identity::new(embedder, Arc::new(store));
        id.add_goal("learn rust", vec![0.5; 128], None);
        assert!(id.top_goal().is_some(), "Should have a top goal");
        assert_eq!(id.top_goal().unwrap().description, "learn rust");
    }
}
