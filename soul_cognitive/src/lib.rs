use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CognitiveError {
    #[error("Entity not found: {0}")]
    EntityNotFound(String),
    #[error("Relation not found: {0}")]
    RelationNotFound(String),
    #[error("Context overflow")]
    ContextOverflow,
}

// ═══════════════════════════════════════════════════════════════
// KNOWLEDGE GRAPH
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: String,
    pub name: String,
    pub entity_type: String,
    pub properties: HashMap<String, serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub relation_type: String,
    pub weight: f32,
    pub properties: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inference {
    pub source: String,
    pub target: String,
    pub relation: String,
    pub confidence: f32,
    pub path: Vec<String>,
}

pub struct KnowledgeGraph {
    entities: HashMap<String, Entity>,
    relations: HashMap<String, Relation>,
    adjacency: HashMap<String, Vec<String>>,
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self {
            entities: HashMap::new(),
            relations: HashMap::new(),
            adjacency: HashMap::new(),
        }
    }

    pub fn add_entity(&mut self, name: &str, entity_type: &str) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let entity = Entity {
            id: id.clone(),
            name: name.to_string(),
            entity_type: entity_type.to_string(),
            properties: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        self.entities.insert(id.clone(), entity);
        id
    }

    pub fn add_relation(&mut self, source_id: &str, target_id: &str, relation_type: &str) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let relation = Relation {
            id: id.clone(),
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            relation_type: relation_type.to_string(),
            weight: 1.0,
            properties: HashMap::new(),
        };
        self.relations.insert(id.clone(), relation);
        self.adjacency
            .entry(source_id.to_string())
            .or_insert_with(Vec::new)
            .push(target_id.to_string());
        id
    }

    pub fn get_entity(&self, id: &str) -> Option<&Entity> {
        self.entities.get(id)
    }

    pub fn get_related(&self, entity_id: &str) -> Vec<&Entity> {
        self.adjacency
            .get(entity_id)
            .map(|ids| ids.iter().filter_map(|id| self.entities.get(id)).collect())
            .unwrap_or_default()
    }

    pub fn find_path(&self, from: &str, to: &str, max_depth: usize) -> Option<Vec<String>> {
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back((from.to_string(), vec![from.to_string()]));
        visited.insert(from.to_string());

        while let Some((current, path)) = queue.pop_front() {
            if path.len() > max_depth {
                continue;
            }
            if current == to {
                return Some(path);
            }
            if let Some(neighbors) = self.adjacency.get(&current) {
                for neighbor in neighbors {
                    if !visited.contains(neighbor) {
                        visited.insert(neighbor.clone());
                        let mut new_path = path.clone();
                        new_path.push(neighbor.clone());
                        queue.push_back((neighbor.clone(), new_path));
                    }
                }
            }
        }
        None
    }

    pub fn infer(&self, entity_id: &str, max_depth: usize) -> Vec<Inference> {
        let mut inferences = Vec::new();
        if let Some(entity) = self.entities.get(entity_id) {
            for other in self.entities.values() {
                if other.id != entity_id {
                    if let Some(path) = self.find_path(entity_id, &other.id, max_depth) {
                        inferences.push(Inference {
                            source: entity.name.clone(),
                            target: other.name.clone(),
                            relation: "connected_via".to_string(),
                            confidence: 1.0 / path.len() as f32,
                            path,
                        });
                    }
                }
            }
        }
        inferences
    }

    pub fn search(&self, query: &str) -> Vec<&Entity> {
        let query_lower = query.to_lowercase();
        self.entities
            .values()
            .filter(|e| {
                e.name.to_lowercase().contains(&query_lower)
                    || e.entity_type.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    pub fn stats(&self) -> serde_json::Value {
        serde_json::json!({
            "entities": self.entities.len(),
            "relations": self.relations.len(),
            "types": self.count_types(),
        })
    }

    fn count_types(&self) -> HashMap<String, usize> {
        let mut types = HashMap::new();
        for entity in self.entities.values() {
            *types.entry(entity.entity_type.clone()).or_insert(0) += 1;
        }
        types
    }
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// LEARNING SYSTEM
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experience {
    pub id: String,
    pub action: String,
    pub context: String,
    pub outcome: String,
    pub reward: f32,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

pub struct LearningSystem {
    experiences: Vec<Experience>,
    patterns: HashMap<String, f32>,
}

impl LearningSystem {
    pub fn new() -> Self {
        Self {
            experiences: Vec::new(),
            patterns: HashMap::new(),
        }
    }

    pub fn record(&mut self, action: &str, context: &str, outcome: &str, reward: f32) {
        let experience = Experience {
            id: uuid::Uuid::new_v4().to_string(),
            action: action.to_string(),
            context: context.to_string(),
            outcome: outcome.to_string(),
            reward,
            timestamp: chrono::Utc::now(),
        };
        self.experiences.push(experience);
        self.update_patterns(action, reward);
    }

    fn update_patterns(&mut self, action: &str, reward: f32) {
        let entry = self.patterns.entry(action.to_string()).or_insert(0.0);
        *entry = (*entry * 0.9) + (reward * 0.1);
    }

    pub fn suggest(&self, context: &str) -> Vec<(String, f32)> {
        let mut suggestions: Vec<(String, f32)> = self.patterns
            .iter()
            .map(|(action, score)| (action.clone(), *score))
            .collect();
        suggestions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        suggestions
    }

    pub fn success_rate(&self) -> f32 {
        if self.experiences.is_empty() {
            return 0.5;
        }
        let positive = self.experiences.iter().filter(|e| e.reward > 0.0).count() as f32;
        positive / self.experiences.len() as f32
    }

    pub fn recent(&self, n: usize) -> &[Experience] {
        let len = self.experiences.len();
        let start = len.saturating_sub(n);
        &self.experiences[start..]
    }
}

impl Default for LearningSystem {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// MULTI-MODEL ROUTER
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    pub endpoint: String,
    pub capabilities: Vec<String>,
    pub max_tokens: usize,
    pub cost_per_1k: f32,
}

pub struct MultiModelRouter {
    models: Vec<ModelConfig>,
    current_model: String,
    performance: HashMap<String, f32>,
}

impl MultiModelRouter {
    pub fn new() -> Self {
        let mut models = Vec::new();
        models.push(ModelConfig {
            name: "qwen3:4b".to_string(),
            endpoint: "http://127.0.0.1:11434".to_string(),
            capabilities: vec!["general".to_string(), "code".to_string()],
            max_tokens: 2048,
            cost_per_1k: 0.0,
        });
        models.push(ModelConfig {
            name: "qwen3.6:35b".to_string(),
            endpoint: "http://127.0.0.1:11434".to_string(),
            capabilities: vec!["general".to_string(), "code".to_string(), "reasoning".to_string()],
            max_tokens: 4096,
            cost_per_1k: 0.0,
        });
        models.push(ModelConfig {
            name: "gemma4:31b".to_string(),
            endpoint: "http://127.0.0.1:11434".to_string(),
            capabilities: vec!["general".to_string(), "code".to_string(), "vision".to_string()],
            max_tokens: 4096,
            cost_per_1k: 0.0,
        });

        Self {
            models,
            current_model: "qwen3:4b".to_string(),
            performance: HashMap::new(),
        }
    }

    pub fn select_model(&self, task_type: &str) -> &ModelConfig {
        self.models
            .iter()
            .find(|m| m.capabilities.contains(&task_type.to_string()))
            .unwrap_or(&self.models[0])
    }

    pub fn record_performance(&mut self, model: &str, score: f32) {
        let entry = self.performance.entry(model.to_string()).or_insert(0.5);
        *entry = (*entry * 0.9) + (score * 0.1);
    }

    pub fn best_model(&self) -> &str {
        self.performance
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(name, _)| name.as_str())
            .unwrap_or("qwen3:4b")
    }

    pub fn list_models(&self) -> &[ModelConfig] {
        &self.models
    }
}

impl Default for MultiModelRouter {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// CONTEXT MANAGER
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEntry {
    pub content: String,
    pub importance: f32,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub access_count: usize,
}

pub struct ContextManager {
    short_term: Vec<ContextEntry>,
    long_term: Vec<ContextEntry>,
    max_short_term: usize,
    max_long_term: usize,
}

impl ContextManager {
    pub fn new(max_short: usize, max_long: usize) -> Self {
        Self {
            short_term: Vec::new(),
            long_term: Vec::new(),
            max_short_term: max_short,
            max_long_term: max_long,
        }
    }

    pub fn add(&mut self, content: &str, importance: f32) {
        let entry = ContextEntry {
            content: content.to_string(),
            importance,
            timestamp: chrono::Utc::now(),
            access_count: 0,
        };
        self.short_term.push(entry);
        if self.short_term.len() > self.max_short_term {
            self.consolidate();
        }
    }

    fn consolidate(&mut self) {
        if let Some(entry) = self.short_term.first() {
            if entry.importance > 0.7 {
                self.long_term.push(entry.clone());
                if self.long_term.len() > self.max_long_term {
                    self.long_term.sort_by(|a, b| b.importance.partial_cmp(&a.importance).unwrap());
                    self.long_term.truncate(self.max_long_term);
                }
            }
        }
        self.short_term.remove(0);
    }

    pub fn recall(&mut self, query: &str) -> Vec<&ContextEntry> {
        let query_lower = query.to_lowercase();
        let mut results: Vec<&ContextEntry> = self.short_term.iter()
            .chain(self.long_term.iter())
            .filter(|e| e.content.to_lowercase().contains(&query_lower))
            .collect();
        results.sort_by(|a, b| b.importance.partial_cmp(&a.importance).unwrap());
        results
    }

    pub fn get_context(&self) -> String {
        let mut context = String::new();
        for entry in self.short_term.iter().rev().take(5) {
            context.push_str(&format!("[{}] {}\n", entry.timestamp.format("%H:%M"), entry.content));
        }
        context
    }

    pub fn stats(&self) -> serde_json::Value {
        serde_json::json!({
            "short_term": self.short_term.len(),
            "long_term": self.long_term.len(),
        })
    }
}

impl Default for ContextManager {
    fn default() -> Self {
        Self::new(20, 100)
    }
}

// ═══════════════════════════════════════════════════════════════
// COGNITIVE ENGINE (ties everything together)
// ═══════════════════════════════════════════════════════════════

pub struct CognitiveEngine {
    pub knowledge: KnowledgeGraph,
    pub learning: LearningSystem,
    pub router: MultiModelRouter,
    pub context: ContextManager,
}

impl CognitiveEngine {
    pub fn new() -> Self {
        Self {
            knowledge: KnowledgeGraph::new(),
            learning: LearningSystem::new(),
            router: MultiModelRouter::new(),
            context: ContextManager::new(20, 100),
        }
    }

    pub fn think(&mut self, input: &str) -> serde_json::Value {
        self.context.add(input, 0.5);
        let relevant = self.knowledge.search(input);
        let suggestions = self.learning.suggest(input);
        let model = self.router.select_model("general");

        serde_json::json!({
            "input": input,
            "relevant_entities": relevant.len(),
            "suggestions": suggestions.len(),
            "model": model.name,
            "context_size": self.context.stats(),
        })
    }

    pub fn learn(&mut self, action: &str, outcome: &str, reward: f32) {
        let context = self.context.get_context();
        self.learning.record(action, &context, outcome, reward);
        let model = self.router.current_model.clone();
        self.router.record_performance(&model, reward);
    }

    pub fn status(&self) -> serde_json::Value {
        serde_json::json!({
            "knowledge": self.knowledge.stats(),
            "learning_rate": self.learning.success_rate(),
            "current_model": self.router.current_model,
            "context": self.context.stats(),
        })
    }
}

impl Default for CognitiveEngine {
    fn default() -> Self {
        Self::new()
    }
}
