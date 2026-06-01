// === Nouveaux traits pour le Système 3 ===

/// Un état cognitif persistant, doté d'une dynamique hamiltonienne.
pub trait CognitiveState: Send + Sync {
    fn as_vector(&self) -> Vec<f64>;
    fn update(&mut self, impulse: &[f64], dt: f64);
    fn distance_to(&self, other: &dyn CognitiveState) -> f64;
}

/// Un critique capable d'évaluer une action ou une situation.
pub trait Critic: Send + Sync {
    fn evaluate(&self, state: &[f64], action: &[f64]) -> f64;
    fn update(&mut self, state: &[f64], action: &[f64], reward: f64, next_state: &[f64]);
}

/// Mémoire narrative : événements, souvenirs, identité.
pub trait NarrativeMemory: Send + Sync {
    fn store(&mut self, episode: NarrativeEpisode);
    fn recall(&self, query: &[f64], k: usize) -> Vec<NarrativeEpisode>;
    fn recent(&self, n: usize) -> Vec<NarrativeEpisode>;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NarrativeEpisode {
    pub timestamp: u64,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub embedding: Vec<f64>,
    pub salience: f64,
    pub outcome: Option<Outcome>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Outcome {
    Success(f64),
    Failure(f64),
}

/// Un but persistant.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Goal {
    pub id: String,
    pub description: String,
    pub embedding: Vec<f64>,
    pub priority: f64,
    pub created_at: u64,
    pub deadline: Option<u64>,
    pub status: GoalStatus,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum GoalStatus {
    Active,
    Suspended,
    Completed,
    Failed,
}

/// Outil actionnable.
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn precondition(&self) -> Vec<Fact>;
    fn postcondition(&self) -> Vec<Fact>;
    fn execute(&self, params: &serde_json::Value) -> Result<serde_json::Value, String>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Fact {
    pub relation: String,
    pub args: Vec<String>,
}
