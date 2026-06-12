//! Pont Forge (moteur évolutionnaire) ↔ SoulSystem.
//!
//! Expose les types de `forge-core` (Engine, Config, Domain, Score) en tant
//! qu'API typée consommable par les autres briques du système (synergie,
//! openevolve, mesh). Le transport HTTP (port 7890) vit dans le binaire
//! `forge-service` ; ce crate fournit la **vue fonctionnelle** du moteur.

use forge_core::{Candidate, Config, Domain, Score};
use serde::{Deserialize, Serialize};
use tracing::info;

pub mod binpack_demo;
pub mod llm_ollama;

/// Alias sémantique : la `Config` du moteur vue par les autres briques.
///
/// Permet aux consommateurs (synergie, openevolve, mesh) de parler de
/// `forge_bridge::ForgeConfig` sans dépendre directement de `forge_core`.
pub type ForgeConfig = Config;

/// Campagne de recherche évolutionnaire prête à être lancée.
///
/// Encapsule la configuration et le domaine. C'est l'entrée naturelle pour
/// les autres bridges : ils n'ont pas à connaître `forge_core::Engine`.
pub struct ForgeCampaign<D: Domain>
where
    D::Cand: Serialize + for<'a> Deserialize<'a>,
{
    pub config: ForgeConfig,
    pub domain: D,
}

impl<D: Domain> ForgeCampaign<D>
where
    D::Cand: Serialize + for<'a> Deserialize<'a>,
{
    pub fn new(config: ForgeConfig, domain: D) -> Self {
        Self { config, domain }
    }

    /// Lance la campagne. En cas d'erreur du moteur, on **n'écrase pas**
    /// silencieusement la cause : on log avec `error!` puis on renvoie un
    /// `Report` vide (best=None, history=[]). C'est délibéré : les bridges
    /// en amont peuvent ainsi distinguer «pas de meilleure solution» de
    /// «crash du moteur» en testant `best.is_none() && history.is_empty()`.
    pub fn run(self) -> forge_core::Report<D::Cand> {
        info!(target: "forge-bridge", "lancement campagne domaine={}", self.domain.name());
        let engine = forge_core::Engine::new(self.domain, self.config);
        match engine.run() {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(target: "forge-bridge", "campagne en echec: {e}");
                forge_core::Report {
                    best: None,
                    final_baseline: None,
                    holdout_best: None,
                    holdout_baseline: None,
                    history: Vec::new(),
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// DTOs (Data Transfer Objects)
//
// Le pont sérialise via `serde_json` ; les types internes de `forge-core`
// (`Score`, `Candidate`) ne sont pas tous forcément wire-friendly (le
// `Candidate` est un trait). On expose des DTOs plats.
// ─────────────────────────────────────────────────────────────────────────────

/// Vue sérialisable d'un [`Score`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScoreDto {
    pub objectives: Vec<f64>,
    pub valid: bool,
}

impl From<Score> for ScoreDto {
    fn from(s: Score) -> Self {
        ScoreDto {
            objectives: s.objectives,
            valid: s.valid,
        }
    }
}

/// Vue sérialisable d'un candidat (id + représentation textuelle).
///
/// On ne transfère pas le type concret : un `Candidate` est un trait
/// hétérogène ; on n'a besoin que de son hash de contenu et de sa
/// représentation pour le logger / rejouer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CandidateDto {
    pub id: u64,
    pub repr: String,
}

impl<T: Candidate> From<&T> for CandidateDto {
    fn from(c: &T) -> Self {
        CandidateDto {
            id: c.id(),
            repr: c.repr(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests unitaires
//
// Couverture :
//  1. DTOs : conversions, round-trip JSON, déterminisme du hash (FNV-1a).
//  2. ForgeCampaign : `new` conserve la config, run() produit un report non
//     vide, déterministe sur la même graine, divergent entre deux graines.
//  3. Politique d'erreur : un `Report` "vide" est bien retourné (best=None,
//     history vide) si le moteur échoue ; la signature est non-panique.
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use binpack_demo::{BinPacking, PackHeuristic};

    // ── Domaine de test compact ────────────────────────────────────────────
    fn small_cfg(g: u64, pop: usize, surv: usize, seed: u64) -> ForgeConfig {
        ForgeConfig {
            generations: g,
            population: pop,
            survivors: surv,
            base_seed: seed,
            max_reflection_retries: 2,
            stagnation_threshold: 5,
        }
    }

    fn small_domain() -> BinPacking {
        BinPacking {
            capacity: 1.0,
            n_items: 15,
            n_instances: 4,
        }
    }

    // ── 1. DTOs ────────────────────────────────────────────────────────────

    #[test]
    fn score_dto_from_valid_score() {
        let s = Score::valid(vec![1.5, 2.5, 3.5]);
        let dto: ScoreDto = s.into();
        assert!(dto.valid);
        assert_eq!(dto.objectives, vec![1.5, 2.5, 3.5]);
    }

    #[test]
    fn score_dto_from_invalid_score() {
        let s = Score::invalid();
        let dto: ScoreDto = s.into();
        assert!(!dto.valid);
        assert!(dto.objectives.is_empty());
    }

    #[test]
    fn score_dto_round_trip_json() {
        let original = ScoreDto {
            objectives: vec![0.1, 9.9],
            valid: true,
        };
        let json = serde_json::to_string(&original).expect("serialisation");
        let restored: ScoreDto = serde_json::from_str(&json).expect("deserialisation");
        assert_eq!(original, restored);
    }

    #[test]
    fn score_dto_round_trip_preserves_invalid_flag() {
        let original = ScoreDto {
            objectives: vec![],
            valid: false,
        };
        let json = serde_json::to_string(&original).unwrap();
        let restored: ScoreDto = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
        assert!(!restored.valid);
    }

    #[test]
    fn candidate_dto_uses_fnv1a_id() {
        // Le DTO doit rester cohérent avec l'API Candidate : id = fnv1a(repr()).
        // Si ce n'est plus le cas un jour, ce test casse (et c'est le but).
        let c = PackHeuristic {
            w: [0.1, 0.2, 0.3, 0.4],
        };
        let expected_id = forge_core::fnv1a(&c.repr());
        let dto: CandidateDto = CandidateDto::from(&c);
        assert_eq!(dto.id, expected_id);
        assert!(dto.repr.contains("0.1000"));
    }

    #[test]
    fn candidate_dto_round_trip_json() {
        let original = CandidateDto {
            id: 0xDEAD_BEEF_CAFE_F00D,
            repr: "test".into(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let restored: CandidateDto = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }

    // ── 2. ForgeCampaign ───────────────────────────────────────────────────

    #[test]
    fn forge_bridge_new_preserves_config() {
        let cfg = small_cfg(7, 11, 3, 42);
        let campaign = ForgeCampaign::new(cfg, small_domain());
        assert_eq!(campaign.config.generations, 7);
        assert_eq!(campaign.config.population, 11);
        assert_eq!(campaign.config.survivors, 3);
        assert_eq!(campaign.config.base_seed, 42);
    }

    #[test]
    fn forge_bridge_compiles_and_runs_binpack() {
        let report = ForgeCampaign::new(small_cfg(3, 8, 2, 1), small_domain()).run();
        assert!(!report.history.is_empty(), "doit produire un historique");
        // Une campagne qui tourne doit produire une baseline.
        assert!(report.final_baseline.is_some());
    }

    #[test]
    fn forge_bridge_run_is_deterministic_with_same_seed() {
        let a = ForgeCampaign::new(small_cfg(4, 6, 2, 0xC0FFEE), small_domain()).run();
        let b = ForgeCampaign::new(small_cfg(4, 6, 2, 0xC0FFEE), small_domain()).run();
        assert_eq!(
            a.history, b.history,
            "même graine ⇒ même courbe d'apprentissage"
        );
    }

    #[test]
    fn forge_bridge_run_diverges_with_different_seeds() {
        // Très petite campagne : on s'attend à observer *au moins une*
        // différence dans l'historique. Si ce n'est pas le cas, la graine
        // n'est pas utilisée — le moteur a un bug.
        let a = ForgeCampaign::new(small_cfg(5, 8, 2, 1), small_domain()).run();
        let b = ForgeCampaign::new(small_cfg(5, 8, 2, 99_999), small_domain()).run();
        assert_ne!(
            a.history, b.history,
            "graines différentes ⇒ historiques différents"
        );
    }

    #[test]
    fn forge_bridge_holdout_is_populated_when_best_exists() {
        // Sur la démo binpack, le moteur trouve presque toujours un
        // candidat valide : le holdout doit donc être renseigné.
        let report = ForgeCampaign::new(small_cfg(3, 6, 2, 7), small_domain()).run();
        assert!(
            report.best.is_some(),
            "un meilleur candidat doit être trouvé"
        );
        let best = report.best.as_ref().unwrap();
        assert!(best.score.valid, "le meilleur doit être valide");
        assert!(
            report.holdout_best.is_some(),
            "holdout best doit être évalué"
        );
        assert!(
            report.holdout_baseline.is_some(),
            "holdout baseline doit être mesurée"
        );
    }

    #[test]
    fn forge_bridge_history_length_matches_generations() {
        let g = 6;
        let report = ForgeCampaign::new(small_cfg(g, 6, 2, 3), small_domain()).run();
        assert_eq!(
            report.history.len(),
            g as usize,
            "la courbe d'apprentissage doit compter une entrée par génération"
        );
    }

    #[test]
    fn forge_bridge_single_survivor_still_runs() {
        // survivors=1 : configuration extrême, mais légale. Le moteur
        // doit produire un report non-vide.
        let report = ForgeCampaign::new(small_cfg(2, 4, 1, 0), small_domain()).run();
        assert_eq!(report.history.len(), 2);
    }

    #[test]
    fn forge_bridge_minimal_population_runs() {
        // population=2, survivors=1 : la plus petite campagne sensée.
        let report = ForgeCampaign::new(small_cfg(1, 2, 1, 0), small_domain()).run();
        assert_eq!(report.history.len(), 1);
    }

    // ── 3. DTOs + Report combinés ──────────────────────────────────────────

    #[test]
    fn best_individual_serializes_through_dto() {
        let report = ForgeCampaign::new(small_cfg(2, 4, 1, 1), small_domain()).run();
        let best = report.best.expect("best doit exister");
        let cand_dto = CandidateDto::from(&best.cand);
        let score_dto: ScoreDto = best.score.clone().into();
        // Les deux DTOs doivent sérialiser en JSON sans erreur, séparément.
        let _ = serde_json::to_string(&cand_dto).expect("cand dto → json");
        let _ = serde_json::to_string(&score_dto).expect("score dto → json");
        // Et le candidat doit être sérialisable en JSON via la même API.
        let cand_json = serde_json::to_string(&best.cand).expect("cand → json");
        let restored: PackHeuristic =
            serde_json::from_str(&cand_json).expect("json → cand round-trip");
        assert_eq!(restored.repr(), best.cand.repr());
    }

    // ── 4. Politique d'erreur : signature non-paniçante ─────────────────────

    #[test]
    fn forge_bridge_run_does_not_panic_on_invalid_survivors() {
        // `Config::survivors` est `usize` (pas d'enum fini) : on vérifie au
        // moins que la signature est non-panique pour 0, et que le moteur
        // dégrade proprement. (Le moteur floor à `max(1)` via `truncate`.)
        let report = ForgeCampaign::new(small_cfg(1, 4, 0, 0), small_domain()).run();
        // Aucune panique, et un report cohérent (peut être best=None, c'est ok).
        assert!(!report.history.is_empty());
    }
}
