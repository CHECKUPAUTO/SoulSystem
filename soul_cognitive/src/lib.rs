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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

    pub fn add_relation(
        &mut self,
        source_id: &str,
        target_id: &str,
        relation_type: &str,
    ) -> String {
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
            .or_default()
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

    pub fn save(&self, path: &str) -> Result<(), String> {
        let data = serde_json::to_string(self).map_err(|e| e.to_string())?;
        std::fs::write(path, data).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn load(path: &str) -> Result<Self, String> {
        let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&data).map_err(|e| e.to_string())
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        let context_lower = context.to_lowercase();
        let context_words: Vec<&str> = context_lower
            .split_whitespace()
            .filter(|w| w.len() > 2)
            .collect();

        let mut suggestions: Vec<(String, f32)> = self
            .patterns
            .iter()
            .map(|(action, score)| {
                // Boost actions lexically related to the current context.
                let action_lower = action.to_lowercase();
                let overlap = context_words
                    .iter()
                    .filter(|w| action_lower.contains(**w))
                    .count() as f32;
                let relevance = 1.0 + (overlap * 0.25).min(1.0);
                (action.clone(), *score * relevance)
            })
            .collect();
        suggestions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
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

    pub fn save(&self, path: &str) -> Result<(), String> {
        let data = serde_json::to_string(self).map_err(|e| e.to_string())?;
        std::fs::write(path, data).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn load(path: &str) -> Result<Self, String> {
        let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&data).map_err(|e| e.to_string())
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
        let models = vec![
            ModelConfig {
                name: "qwen3:4b".to_string(),
                endpoint: "http://127.0.0.1:11434".to_string(),
                capabilities: vec!["general".to_string(), "code".to_string()],
                max_tokens: 2048,
                cost_per_1k: 0.0,
            },
            ModelConfig {
                name: "qwen3.6:35b".to_string(),
                endpoint: "http://127.0.0.1:11434".to_string(),
                capabilities: vec![
                    "general".to_string(),
                    "code".to_string(),
                    "reasoning".to_string(),
                ],
                max_tokens: 4096,
                cost_per_1k: 0.0,
            },
            ModelConfig {
                name: "gemma4:31b".to_string(),
                endpoint: "http://127.0.0.1:11434".to_string(),
                capabilities: vec![
                    "general".to_string(),
                    "code".to_string(),
                    "vision".to_string(),
                ],
                max_tokens: 4096,
                cost_per_1k: 0.0,
            },
        ];

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
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
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
                    self.long_term.sort_by(|a, b| {
                        b.importance
                            .partial_cmp(&a.importance)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    self.long_term.truncate(self.max_long_term);
                }
            }
        }
        self.short_term.remove(0);
    }

    pub fn recall(&mut self, query: &str) -> Vec<&ContextEntry> {
        let query_lower = query.to_lowercase();
        let mut results: Vec<&ContextEntry> = self
            .short_term
            .iter()
            .chain(self.long_term.iter())
            .filter(|e| e.content.to_lowercase().contains(&query_lower))
            .collect();
        results.sort_by(|a, b| {
            b.importance
                .partial_cmp(&a.importance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    pub fn get_context(&self) -> String {
        let mut context = String::new();
        for entry in self.short_term.iter().rev().take(5) {
            context.push_str(&format!(
                "[{}] {}\n",
                entry.timestamp.format("%H:%M"),
                entry.content
            ));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_knowledge_graph_add_entity() {
        let mut kg = KnowledgeGraph::new();
        let id = kg.add_entity("TestEntity", "concept");
        assert!(kg.get_entity(&id).is_some());
        assert_eq!(kg.get_entity(&id).unwrap().name, "TestEntity");
    }

    #[test]
    fn test_knowledge_graph_add_relation() {
        let mut kg = KnowledgeGraph::new();
        let a = kg.add_entity("A", "node");
        let b = kg.add_entity("B", "node");
        let rel_id = kg.add_relation(&a, &b, "connects_to");
        assert!(!rel_id.is_empty());
        let related = kg.get_related(&a);
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].name, "B");
    }

    #[test]
    fn test_knowledge_graph_find_path() {
        let mut kg = KnowledgeGraph::new();
        let a = kg.add_entity("A", "node");
        let b = kg.add_entity("B", "node");
        let c = kg.add_entity("C", "node");
        kg.add_relation(&a, &b, "link");
        kg.add_relation(&b, &c, "link");
        let path = kg.find_path(&a, &c, 5);
        assert!(path.is_some());
        assert_eq!(path.unwrap().len(), 3);
    }

    #[test]
    fn test_knowledge_graph_search() {
        let mut kg = KnowledgeGraph::new();
        kg.add_entity("RustLanguage", "language");
        kg.add_entity("Python", "language");
        let results = kg.search("Rust");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "RustLanguage");
    }

    #[test]
    fn test_knowledge_graph_stats() {
        let mut kg = KnowledgeGraph::new();
        kg.add_entity("A", "type1");
        kg.add_entity("B", "type2");
        let stats = kg.stats();
        assert_eq!(stats["entities"], 2);
    }

    #[test]
    fn test_learning_system_record() {
        let mut ls = LearningSystem::new();
        ls.record("action1", "ctx", "success", 1.0);
        ls.record("action2", "ctx", "failure", -1.0);
        assert_eq!(ls.recent(10).len(), 2);
    }

    #[test]
    fn test_learning_system_suggest() {
        let mut ls = LearningSystem::new();
        ls.record("good_action", "ctx", "success", 1.0);
        ls.record("bad_action", "ctx", "failure", -1.0);
        let suggestions = ls.suggest("ctx");
        assert!(!suggestions.is_empty());
    }

    #[test]
    fn test_learning_system_success_rate() {
        let mut ls = LearningSystem::new();
        ls.record("a", "c", "o", 1.0);
        ls.record("b", "c", "o", -1.0);
        assert_eq!(ls.success_rate(), 0.5);
    }

    #[test]
    fn test_learning_system_empty() {
        let ls = LearningSystem::new();
        assert_eq!(ls.success_rate(), 0.5);
    }

    #[test]
    fn test_multi_model_router() {
        let router = MultiModelRouter::new();
        assert!(!router.list_models().is_empty());
        let model = router.select_model("general");
        assert!(!model.name.is_empty());
    }

    #[test]
    fn test_context_manager() {
        let mut ctx = ContextManager::new(5, 10);
        ctx.add("test1", 0.5);
        ctx.add("test2", 0.8);
        assert_eq!(ctx.stats()["short_term"], 2);
        let context = ctx.get_context();
        assert!(context.contains("test1"));
    }

    #[test]
    fn test_context_manager_recall() {
        let mut ctx = ContextManager::new(5, 10);
        ctx.add("important fact", 0.9);
        ctx.add("other thing", 0.3);
        let results = ctx.recall("important");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_cognitive_engine_think() {
        let mut engine = CognitiveEngine::new();
        let result = engine.think("test input");
        assert!(result.get("input").is_some());
    }

    #[test]
    fn test_cognitive_engine_learn() {
        let mut engine = CognitiveEngine::new();
        engine.learn("action", "outcome", 0.8);
        assert_eq!(engine.learning.recent(10).len(), 1);
    }

    #[test]
    fn test_cognitive_engine_status() {
        let engine = CognitiveEngine::new();
        let status = engine.status();
        assert!(status.get("knowledge").is_some());
    }

    #[test]
    fn test_knowledge_graph_save_load() {
        let mut kg = KnowledgeGraph::new();
        let id = kg.add_entity("TestEntity", "concept");
        kg.add_entity("Other", "type");
        let path = "/tmp/soulsystem_test_kg.json";
        kg.save(path).unwrap();
        let loaded = KnowledgeGraph::load(path).unwrap();
        assert_eq!(loaded.entities.len(), 2);
        assert_eq!(loaded.get_entity(&id).unwrap().name, "TestEntity");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_learning_system_save_load() {
        let mut ls = LearningSystem::new();
        ls.record("action1", "ctx", "success", 1.0);
        ls.record("action2", "ctx", "failure", -1.0);
        let path = "/tmp/soulsystem_test_ls.json";
        ls.save(path).unwrap();
        let loaded = LearningSystem::load(path).unwrap();
        assert_eq!(loaded.recent(10).len(), 2);
        assert_eq!(loaded.success_rate(), 0.5);
        let _ = std::fs::remove_file(path);
    }
}
