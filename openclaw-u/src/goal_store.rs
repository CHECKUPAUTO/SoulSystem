//! Persistent goal store (claude-goal compat)
//! External agents (Claude Code, scripts) write goals to ~/.soulsystem/goal.json.
//! openclaw-u reads and processes them each heartbeat cycle.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const GOAL_FILE: &str = ".soulsystem/goal.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalGoal {
    pub id: String,
    pub text: String,
    pub status: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalFile {
    pub goal: Option<ExternalGoal>,
    #[serde(default)]
    pub history: Vec<ExternalGoal>,
}

impl GoalFile {
    fn path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
        Path::new(&home).join(GOAL_FILE)
    }

    pub fn load() -> Self {
        let path = Self::path();
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(gf) = serde_json::from_str::<Self>(&data) {
                return gf;
            }
        }
        Self {
            goal: None,
            history: vec![],
        }
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(&path, json);
        }
    }

    /// Returns the active external goal text, if any.
    pub fn active_goal_text() -> Option<String> {
        let gf = Self::load();
        gf.goal.filter(|g| g.status == "active").map(|g| g.text)
    }

    /// Mark the current external goal as completed (moves to history).
    pub fn complete_active() {
        let mut gf = Self::load();
        if let Some(goal) = gf.goal.take() {
            let completed = ExternalGoal {
                status: "completed".into(),
                ..goal
            };
            gf.history.push(completed);
            if gf.history.len() > 100 {
                gf.history.remove(0);
            }
        }
        gf.save();
    }
}
