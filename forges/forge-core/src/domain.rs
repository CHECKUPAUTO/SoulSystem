use crate::candidate::Candidate;
use crate::error::Result;
use crate::trial::Trial;
use rand::rngs::StdRng;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Score {
    pub objectives: Vec<f64>,
    pub valid: bool,
    pub error_msg: Option<String>,
}

impl Score {
    pub fn invalid() -> Self {
        Score { objectives: Vec::new(), valid: false, error_msg: None }
    }
    pub fn with_error(msg: String) -> Self {
        Score { objectives: Vec::new(), valid: false, error_msg: Some(msg) }
    }
    pub fn valid(objectives: Vec<f64>) -> Self {
        Score { objectives, valid: true, error_msg: None }
    }
    pub fn dominates(&self, other: &Self) -> bool {
        if !self.valid { return false; }
        if !other.valid { return true; }
        let mut better = false;
        for (a, b) in self.objectives.iter().zip(other.objectives.iter()) {
            if a > b { return false; }
            if a < b { better = true; }
        }
        better
    }
}

pub trait Domain: Sync + Send {
    type Cand: Candidate + Clone + serde::Serialize + for<'a> serde::Deserialize<'a> + Send + Sync;

    /// Nom court du domaine (utilisé pour les logs et checkpoints).
    fn name(&self) -> &str;

    fn seed(&self, rng: &mut StdRng) -> Self::Cand;
    fn mutate(&self, rng: &mut StdRng, parents: &[&Self::Cand]) -> Result<Self::Cand>;
    fn verify(&self, cand: &Self::Cand, trial: &Trial) -> Result<bool>;
    fn measure(&self, cand: &Self::Cand, trial: &Trial) -> Result<Vec<f64>>;
    fn objective_names(&self) -> Vec<String>;
    fn baseline(&self, trial: &Trial) -> Result<Score>;

    fn warm_start(&self) -> Vec<Self::Cand> {
        Vec::new()
    }

    fn reflect(&self, _rng: &mut StdRng, _failed: &Self::Cand, _error: &str) -> Result<Option<Self::Cand>> {
        Ok(None)
    }
}
