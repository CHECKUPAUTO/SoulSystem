use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

// ─── Errors ──────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum DesignTreeError {
    #[error("Node not found: {0}")]
    NotFound(String),
    #[error("Invalid transition: {0}")]
    InvalidTransition(String),
    #[error("IO error: {0}")]
    Io(String),
    #[error("Serialization error: {0}")]
    Serde(String),
}

pub type Result<T> = std::result::Result<T, DesignTreeError>;

// ─── Design States (OpenSpec Lifecycle) ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DesignState {
    Idea,
    Research,
    Decision,
    Spec,
    Implementation,
    Testing,
    Verified,
    Abandoned,
}

impl DesignState {
    pub fn valid_transitions(&self) -> Vec<DesignState> {
        match self {
            Self::Idea => vec![Self::Research, Self::Abandoned],
            Self::Research => vec![Self::Decision, Self::Abandoned],
            Self::Decision => vec![Self::Spec, Self::Abandoned],
            Self::Spec => vec![Self::Implementation, Self::Abandoned],
            Self::Implementation => vec![Self::Testing, Self::Abandoned],
            Self::Testing => vec![Self::Verified, Self::Implementation],
            Self::Verified => vec![],
            Self::Abandoned => vec![],
        }
    }

    pub fn can_transition_to(&self, target: &DesignState) -> bool {
        self.valid_transitions().contains(target)
    }

    pub fn all_states() -> Vec<DesignState> {
        vec![
            Self::Idea,
            Self::Research,
            Self::Decision,
            Self::Spec,
            Self::Implementation,
            Self::Testing,
            Self::Verified,
            Self::Abandoned,
        ]
    }
}

// ─── Design Node ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignNode {
    pub id: String,
    pub name: String,
    pub description: String,
    pub state: DesignState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
    #[serde(default)]
    pub state_history: Vec<StateTransition>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub name: String,
    pub path: String,
    pub artifact_type: ArtifactType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArtifactType {
    Spec,
    Code,
    Test,
    Document,
    Mockup,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransition {
    pub from: DesignState,
    pub to: DesignState,
    pub timestamp: DateTime<Utc>,
    pub reason: Option<String>,
}

impl DesignNode {
    pub fn new(name: &str, description: &str) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: description.into(),
            state: DesignState::Idea,
            created_at: now,
            updated_at: now,
            owner: None,
            priority: 5,
            tags: Vec::new(),
            dependencies: Vec::new(),
            artifacts: Vec::new(),
            state_history: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn transition(&mut self, target: DesignState, reason: Option<String>) -> Result<()> {
        if !self.state.can_transition_to(&target) {
            return Err(DesignTreeError::InvalidTransition(format!(
                "cannot go from {:?} to {:?}",
                self.state, target
            )));
        }

        let transition = StateTransition {
            from: self.state.clone(),
            to: target.clone(),
            timestamp: Utc::now(),
            reason,
        };

        self.state_history.push(transition);
        self.state = target;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn add_artifact(&mut self, name: &str, path: &str, artifact_type: ArtifactType) {
        self.artifacts.push(Artifact {
            name: name.into(),
            path: path.into(),
            artifact_type,
        });
        self.updated_at = Utc::now();
    }

    pub fn add_dependency(&mut self, node_id: &str) {
        if !self.dependencies.contains(&node_id.to_string()) {
            self.dependencies.push(node_id.into());
        }
    }
}

// ─── Design Tree ─────────────────────────────────────────────────────────────

pub struct DesignTree {
    nodes: HashMap<String, DesignNode>,
    base_path: PathBuf,
}

impl DesignTree {
    pub fn new(base_path: &Path) -> Self {
        Self {
            nodes: HashMap::new(),
            base_path: base_path.to_path_buf(),
        }
    }

    pub fn create_node(&mut self, name: &str, description: &str) -> String {
        let node = DesignNode::new(name, description);
        let id = node.id.clone();
        self.nodes.insert(id.clone(), node);
        id
    }

    pub fn get_node(&self, id: &str) -> Option<&DesignNode> {
        self.nodes.get(id)
    }

    pub fn get_node_mut(&mut self, id: &str) -> Option<&mut DesignNode> {
        self.nodes.get_mut(id)
    }

    pub fn transition_node(
        &mut self,
        id: &str,
        target: DesignState,
        reason: Option<String>,
    ) -> Result<()> {
        let node = self
            .nodes
            .get_mut(id)
            .ok_or_else(|| DesignTreeError::NotFound(id.into()))?;
        node.transition(target, reason)
    }

    pub fn find_by_state(&self, state: &DesignState) -> Vec<&DesignNode> {
        self.nodes.values().filter(|n| n.state == *state).collect()
    }

    pub fn find_by_name(&self, name: &str) -> Vec<&DesignNode> {
        self.nodes
            .values()
            .filter(|n| n.name.to_lowercase().contains(&name.to_lowercase()))
            .collect()
    }

    pub fn find_by_tag(&self, tag: &str) -> Vec<&DesignNode> {
        self.nodes
            .values()
            .filter(|n| n.tags.iter().any(|t| t == tag))
            .collect()
    }

    pub fn blocked_by(&self, node_id: &str) -> Vec<&DesignNode> {
        if let Some(node) = self.nodes.get(node_id) {
            node.dependencies
                .iter()
                .filter_map(|dep| self.nodes.get(dep))
                .collect()
        } else {
            vec![]
        }
    }

    pub fn dependents(&self, node_id: &str) -> Vec<&DesignNode> {
        self.nodes
            .values()
            .filter(|n| n.dependencies.contains(&node_id.to_string()))
            .collect()
    }

    pub fn stats(&self) -> DesignTreeStats {
        let mut state_counts: HashMap<String, usize> = HashMap::new();
        for node in self.nodes.values() {
            let state_name = format!("{:?}", node.state);
            *state_counts.entry(state_name).or_insert(0) += 1;
        }

        let active = self
            .nodes
            .values()
            .filter(|n| !matches!(n.state, DesignState::Verified | DesignState::Abandoned))
            .count();

        let completed = self
            .nodes
            .values()
            .filter(|n| matches!(n.state, DesignState::Verified))
            .count();

        DesignTreeStats {
            total: self.nodes.len(),
            active,
            completed,
            abandoned: self
                .nodes
                .values()
                .filter(|n| matches!(n.state, DesignState::Abandoned))
                .count(),
            state_counts,
        }
    }

    pub async fn save(&self) -> Result<()> {
        let dir = &self.base_path;
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|e| DesignTreeError::Io(e.to_string()))?;

        for node in self.nodes.values() {
            let path = dir.join(format!("{}.json", node.id));
            let json = serde_json::to_string_pretty(node)
                .map_err(|e| DesignTreeError::Serde(e.to_string()))?;
            tokio::fs::write(&path, json)
                .await
                .map_err(|e| DesignTreeError::Io(e.to_string()))?;
        }

        Ok(())
    }

    pub async fn load(&mut self) -> Result<usize> {
        let dir = &self.base_path;
        if !dir.exists() {
            return Ok(0);
        }

        let mut count = 0;
        let mut entries = tokio::fs::read_dir(dir)
            .await
            .map_err(|e| DesignTreeError::Io(e.to_string()))?;

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(content) = tokio::fs::read_to_string(&path).await {
                    if let Ok(node) = serde_json::from_str::<DesignNode>(&content) {
                        self.nodes.insert(node.id.clone(), node);
                        count += 1;
                    }
                }
            }
        }

        Ok(count)
    }
}

// ─── Stats ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignTreeStats {
    pub total: usize,
    pub active: usize,
    pub completed: usize,
    pub abandoned: usize,
    pub state_counts: HashMap<String, usize>,
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_design_state_transitions() {
        assert!(DesignState::Idea.can_transition_to(&DesignState::Research));
        assert!(DesignState::Idea.can_transition_to(&DesignState::Abandoned));
        assert!(!DesignState::Idea.can_transition_to(&DesignState::Verified));
        assert!(!DesignState::Verified.can_transition_to(&DesignState::Idea));
    }

    #[test]
    fn test_node_transition() {
        let mut node = DesignNode::new("test", "desc");
        assert!(node.transition(DesignState::Research, None).is_ok());
        assert!(node.transition(DesignState::Decision, None).is_ok());
        assert!(!node.transition(DesignState::Idea, None).is_ok());
    }

    #[test]
    fn test_design_tree_create() {
        let dir = tempfile::tempdir().unwrap();
        let mut tree = DesignTree::new(dir.path());
        let id = tree.create_node("feature X", "A new feature");
        assert!(tree.get_node(&id).is_some());
    }

    #[test]
    fn test_find_by_state() {
        let dir = tempfile::tempdir().unwrap();
        let mut tree = DesignTree::new(dir.path());
        let id1 = tree.create_node("A", "desc A");
        let id2 = tree.create_node("B", "desc B");
        tree.transition_node(&id1, DesignState::Research, None)
            .unwrap();

        let ideas = tree.find_by_state(&DesignState::Idea);
        assert_eq!(ideas.len(), 1);
        assert_eq!(ideas[0].id, id2);

        let researching = tree.find_by_state(&DesignState::Research);
        assert_eq!(researching.len(), 1);
    }

    #[test]
    fn test_dependency_tracking() {
        let dir = tempfile::tempdir().unwrap();
        let mut tree = DesignTree::new(dir.path());
        let id1 = tree.create_node("A", "desc A");
        let id2 = tree.create_node("B", "desc B");
        tree.get_node_mut(&id2).unwrap().add_dependency(&id1);

        let blocked = tree.blocked_by(&id2);
        assert_eq!(blocked.len(), 1);
    }

    #[test]
    fn test_stats() {
        let dir = tempfile::tempdir().unwrap();
        let mut tree = DesignTree::new(dir.path());
        let id = tree.create_node("A", "desc");
        tree.transition_node(&id, DesignState::Research, None)
            .unwrap();
        let stats = tree.stats();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.active, 1);
    }

    #[test]
    fn test_save_load() {
        let dir = tempfile::tempdir().unwrap();
        let mut tree = DesignTree::new(dir.path());
        tree.create_node("A", "desc A");
        tree.create_node("B", "desc B");

        // Save (async)
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(tree.save()).unwrap();

        let mut loaded = DesignTree::new(dir.path());
        let count = rt.block_on(loaded.load()).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let mut tree = DesignTree::new(dir.path());
        let id = tree.create_node("A", "desc");
        tree.get_node_mut(&id)
            .unwrap()
            .add_artifact("spec.md", "specs/A.md", ArtifactType::Spec);
        assert_eq!(tree.get_node(&id).unwrap().artifacts.len(), 1);
    }

    #[test]
    fn test_abandoned_blocks_further_transitions() {
        let mut node = DesignNode::new("test", "desc");
        node.transition(DesignState::Abandoned, None).unwrap();
        assert!(!node.transition(DesignState::Research, None).is_ok());
    }
}
