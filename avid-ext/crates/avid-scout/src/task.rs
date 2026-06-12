use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoutTask {
    pub url: String,
    pub content: String,
}

impl ScoutTask {
    pub fn new(url: String, content: String) -> Self {
        Self { url, content }
    }
}
