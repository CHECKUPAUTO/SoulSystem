use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodicMemory {
    pub session_id: Uuid,
    pub interactions: Vec<Interaction>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interaction {
    pub timestamp: DateTime<Utc>,
    pub role: String,
    pub content: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticFact {
    pub key: String,
    pub value: String,
    pub confidence: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProceduralSkill {
    pub name: String,
    pub steps: Vec<String>,
}

/// Multi-faceted memory system for the reasoning engine.
#[derive(Default)]
pub struct MemorySystem {
    pub episodic: HashMap<Uuid, EpisodicMemory>,
    pub semantic: Vec<SemanticFact>,
    pub procedural: Vec<ProceduralSkill>,
}

impl MemorySystem {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add_interaction(&mut self, session_id: Uuid, role: String, content: String) {
        let entry = self
            .episodic
            .entry(session_id)
            .or_insert_with(|| EpisodicMemory {
                session_id,
                interactions: Vec::new(),
            });
        entry.interactions.push(Interaction {
            timestamp: Utc::now(),
            role,
            content,
        });
    }
    pub fn learn_fact(&mut self, key: String, value: String, confidence: f32) {
        self.semantic.push(SemanticFact {
            key,
            value,
            confidence,
        });
    }
    pub fn register_skill(&mut self, name: String, steps: Vec<String>) {
        self.procedural.push(ProceduralSkill { name, steps });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_memory_interaction() {
        let mut mem = MemorySystem::new();
        let sid = Uuid::new_v4();
        mem.add_interaction(sid, "user".to_string(), "hello".to_string());
        assert_eq!(mem.episodic.get(&sid).unwrap().interactions.len(), 1);
    }

    #[test]
    fn test_memory_semantic() {
        let mut mem = MemorySystem::new();
        mem.learn_fact("rust".to_string(), "safe".to_string(), 1.0);
        assert_eq!(mem.semantic.len(), 1);
    }

    #[test]
    fn test_memory_procedural() {
        let mut mem = MemorySystem::new();
        mem.register_skill("greet".to_string(), vec!["say hi".to_string()]);
        assert_eq!(mem.procedural.len(), 1);
    }
}
