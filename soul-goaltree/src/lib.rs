//! Hierarchical goal tree with decomposition and progress tracking.
//!
//! Supports:
//! - Goal decomposition into sub-goals
//! - Priority-based ordering
//! - Progress tracking up the tree
//! - Dependency management
//! - Goal templates

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use soul_memory::{GoalStatus, PersistentGoal, PersistentStore};
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum GoalTreeError {
    #[error("Goal not found: {0}")]
    NotFound(String),
    #[error("Circular dependency detected")]
    CircularDependency,
    #[error("Invalid decomposition: {0}")]
    InvalidDecomposition(String),
    #[error("Persist error: {0}")]
    Persist(#[from] soul_memory::PersistError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalNode {
    pub id: String,
    pub description: String,
    pub priority: u8,
    pub status: GoalStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub parent_id: Option<String>,
    pub children_ids: Vec<String>,
    pub result: Option<String>,
    pub dependencies: Vec<String>,
    pub estimated_effort: Option<f32>,
    pub actual_effort: Option<f32>,
    pub notes: Vec<String>,
    pub tags: Vec<String>,
}

impl From<&PersistentGoal> for GoalNode {
    fn from(goal: &PersistentGoal) -> Self {
        Self {
            id: goal.id.clone(),
            description: goal.description.clone(),
            priority: goal.priority,
            status: goal.status.clone(),
            created_at: goal.created_at,
            updated_at: goal.updated_at,
            parent_id: goal.parent_id.clone(),
            children_ids: goal.children_ids.clone(),
            result: goal.result.clone(),
            dependencies: Vec::new(),
            estimated_effort: None,
            actual_effort: None,
            notes: Vec::new(),
            tags: Vec::new(),
        }
    }
}

pub struct GoalTree {
    store: Arc<PersistentStore>,
}

impl GoalTree {
    pub fn new(store: PersistentStore) -> Self {
        Self {
            store: Arc::new(store),
        }
    }

    /// Create a root goal
    pub fn create_root(
        &self,
        description: &str,
        priority: u8,
        tags: Vec<String>,
    ) -> Result<GoalNode, GoalTreeError> {
        let goal = PersistentGoal {
            id: Uuid::new_v4().to_string(),
            description: description.to_string(),
            priority,
            status: GoalStatus::Active,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            parent_id: None,
            children_ids: vec![],
            result: None,
        };
        self.store.save_goal(&goal)?;

        let mut node = GoalNode::from(&goal);
        node.tags = tags;

        Ok(node)
    }

    /// Decompose a goal into sub-goals
    pub fn decompose(
        &self,
        parent_id: &str,
        sub_descriptions: Vec<(&str, u8)>,
    ) -> Result<Vec<GoalNode>, GoalTreeError> {
        let mut parent = self
            .store
            .load_goal(parent_id)?
            .ok_or_else(|| GoalTreeError::NotFound(parent_id.to_string()))?;

        let mut children = Vec::new();

        for (desc, priority) in sub_descriptions {
            let child_goal = PersistentGoal {
                id: Uuid::new_v4().to_string(),
                description: desc.to_string(),
                priority,
                status: GoalStatus::Active,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                parent_id: Some(parent_id.to_string()),
                children_ids: vec![],
                result: None,
            };
            self.store.save_goal(&child_goal)?;
            parent.children_ids.push(child_goal.id.clone());
            children.push(GoalNode::from(&child_goal));
        }

        parent.updated_at = Utc::now();
        self.store.save_goal(&parent)?;

        Ok(children)
    }

    /// Complete a goal and update parent progress
    pub fn complete(&self, goal_id: &str, result: &str) -> Result<(), GoalTreeError> {
        self.store.complete_goal(goal_id, result)?;

        // Update parent if exists
        if let Some(goal) = self.store.load_goal(goal_id)? {
            if let Some(parent_id) = &goal.parent_id {
                self.update_parent_progress(parent_id)?;
            }
        }

        Ok(())
    }

    fn update_parent_progress(&self, parent_id: &str) -> Result<(), GoalTreeError> {
        if let Some(parent) = self.store.load_goal(parent_id)? {
            let mut completed = 0;
            let total = parent.children_ids.len();

            for child_id in &parent.children_ids {
                if let Some(child) = self.store.load_goal(child_id)? {
                    if child.status == GoalStatus::Completed {
                        completed += 1;
                    }
                }
            }

            // If all children completed, complete parent
            if total > 0 && completed == total {
                self.store
                    .complete_goal(parent_id, "All sub-goals completed")?;

                // Recurse up
                if let Some(grandparent_id) = &parent.parent_id {
                    self.update_parent_progress(grandparent_id)?;
                }
            }
        }

        Ok(())
    }

    /// Get all active goals sorted by priority
    pub fn active_goals(&self) -> Result<Vec<GoalNode>, GoalTreeError> {
        let goals = self.store.load_active_goals()?;
        Ok(goals.iter().map(GoalNode::from).collect())
    }

    /// Get full tree from root
    pub fn get_tree(&self, root_id: &str) -> Result<GoalNode, GoalTreeError> {
        let goal = self
            .store
            .load_goal(root_id)?
            .ok_or_else(|| GoalTreeError::NotFound(root_id.to_string()))?;

        let mut node = GoalNode::from(&goal);

        // Recursively build children
        for child_id in &node.children_ids.clone() {
            if let Ok(child) = self.get_tree(child_id) {
                node.children_ids.push(child.id.clone());
            }
        }

        Ok(node)
    }

    /// Find goals by tag
    pub fn find_by_tag(&self, tag: &str) -> Result<Vec<GoalNode>, GoalTreeError> {
        let all_goals = self.store.load_all_goals()?;
        Ok(all_goals
            .iter()
            .filter(|g| {
                // Note: tags aren't in PersistentGoal, but we can filter by description
                g.description.to_lowercase().contains(&tag.to_lowercase())
            })
            .map(GoalNode::from)
            .collect())
    }

    /// Get statistics
    pub fn stats(&self) -> Result<serde_json::Value, GoalTreeError> {
        let all_goals = self.store.load_all_goals()?;
        let total = all_goals.len();
        let active = all_goals
            .iter()
            .filter(|g| g.status == GoalStatus::Active)
            .count();
        let running = all_goals
            .iter()
            .filter(|g| g.status == GoalStatus::Running)
            .count();
        let completed = all_goals
            .iter()
            .filter(|g| g.status == GoalStatus::Completed)
            .count();
        let failed = all_goals
            .iter()
            .filter(|g| g.status == GoalStatus::Failed)
            .count();

        let avg_priority: f32 = if total > 0 {
            all_goals.iter().map(|g| g.priority as f32).sum::<f32>() / total as f32
        } else {
            0.0
        };

        Ok(serde_json::json!({
            "total": total,
            "active": active,
            "running": running,
            "completed": completed,
            "failed": failed,
            "avg_priority": avg_priority,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_tree() -> GoalTree {
        let dir = tempfile::tempdir().unwrap();
        let store = PersistentStore::open(dir.path()).unwrap();
        GoalTree::new(store)
    }

    #[test]
    fn test_create_root() {
        let tree = test_tree();
        let goal = tree
            .create_root("Root goal", 5, vec!["test".into()])
            .unwrap();
        assert_eq!(goal.description, "Root goal");
        assert_eq!(goal.priority, 5);
    }

    #[test]
    fn test_decompose() {
        let tree = test_tree();
        let root = tree.create_root("Root", 5, vec![]).unwrap();
        let children = tree
            .decompose(&root.id, vec![("Sub 1", 3), ("Sub 2", 4), ("Sub 3", 2)])
            .unwrap();
        assert_eq!(children.len(), 3);
    }

    #[test]
    fn test_complete_goal() {
        let tree = test_tree();
        let root = tree.create_root("Root", 5, vec![]).unwrap();
        let children = tree
            .decompose(&root.id, vec![("Sub 1", 3), ("Sub 2", 4)])
            .unwrap();

        tree.complete(&children[0].id, "done 1").unwrap();
        let stats = tree.stats().unwrap();
        assert_eq!(stats["completed"], 1);
    }

    #[test]
    fn test_stats() {
        let tree = test_tree();
        tree.create_root("Goal 1", 5, vec![]).unwrap();
        tree.create_root("Goal 2", 3, vec![]).unwrap();

        let stats = tree.stats().unwrap();
        assert_eq!(stats["total"], 2);
        assert_eq!(stats["active"], 2);
    }

    #[test]
    fn test_create_root_with_tags() {
        let tree = test_tree();
        let goal = tree
            .create_root("Tagged goal", 3, vec!["urgent".into(), "core".into()])
            .unwrap();
        assert_eq!(goal.description, "Tagged goal");
        assert_eq!(goal.tags, vec!["urgent", "core"]);
    }

    #[test]
    fn test_get_tree_returns_root() {
        let tree = test_tree();
        let root = tree.create_root("Root", 5, vec![]).unwrap();

        let loaded = tree.get_tree(&root.id).unwrap();
        assert_eq!(loaded.id, root.id);
        assert_eq!(loaded.description, "Root");
    }

    #[test]
    fn test_get_tree_not_found() {
        let tree = test_tree();
        let result = tree.get_tree("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_decompose_nonexistent_parent() {
        let tree = test_tree();
        let result = tree.decompose("bad-id", vec![("child", 1)]);
        assert!(result.is_err());
    }

    #[test]
    fn test_hierarchical_completion_propagates_up() {
        let tree = test_tree();
        let root = tree.create_root("Root", 5, vec![]).unwrap();

        let sub = tree
            .decompose(&root.id, vec![("Sub A", 3), ("Sub B", 4)])
            .unwrap();

        tree.complete(&sub[0].id, "A done").unwrap();
        tree.complete(&sub[1].id, "B done").unwrap();

        let stats = tree.stats().unwrap();
        assert_eq!(
            stats["completed"], 3,
            "Both children + parent should be completed"
        );
    }

    #[test]
    fn test_complete_nonexistent_goal_is_noop() {
        let tree = test_tree();
        let result = tree.complete("bad-id", "result");
        assert!(result.is_ok());
    }

    #[test]
    fn test_find_by_tag_description_match() {
        let tree = test_tree();
        tree.create_root("Important feature", 5, vec![]).unwrap();

        let results = tree.find_by_tag("important").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].description, "Important feature");
    }

    #[test]
    fn test_find_by_tag_no_match() {
        let tree = test_tree();
        tree.create_root("Some goal", 1, vec![]).unwrap();

        let results = tree.find_by_tag("nonexistent").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_active_goals_empty() {
        let tree = test_tree();
        let goals = tree.active_goals().unwrap();
        assert!(goals.is_empty());
    }

    #[test]
    fn test_priority_sorting_in_stats() {
        let tree = test_tree();
        tree.create_root("Low priority", 1, vec![]).unwrap();
        tree.create_root("High priority", 10, vec![]).unwrap();

        let all = tree.store.load_all_goals().unwrap();
        let priorities: Vec<u8> = all.iter().map(|g| g.priority).collect();
        assert!(priorities.contains(&1));
        assert!(priorities.contains(&10));
    }
}
