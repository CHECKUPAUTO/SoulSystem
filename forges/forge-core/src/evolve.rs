use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::RwLock;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::candidate::Candidate;
use crate::domain::{Domain, Score};
use crate::error::{ForgeError, Result};
use crate::trial::Trial;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Config {
    pub generations: u64,
    pub population: usize,
    pub survivors: usize,
    pub base_seed: u64,
    pub max_reflection_retries: usize,
    pub stagnation_threshold: u64,
}

#[derive(Clone)]
pub struct Individual<C: Candidate> {
    pub cand: C,
    pub score: Score,
    /// Crowding distance NSGA-II. Ignorée à la sérialisation
    /// (c'est un attribut de tri intra-front, pas d'état persistant) ;
    /// l'implémentation manuelle de `Serialize` ne l'émet pas.
    pub crowding_distance: f64,
}

impl<C: Candidate + Serialize + for<'a> Deserialize<'a>> Serialize for Individual<C> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("Individual", 2)?;
        st.serialize_field("cand", &self.cand)?;
        st.serialize_field("score", &self.score)?;
        // crowding_distance est #[serde(skip)] : volontairement non émis.
        st.end()
    }
}

impl<'de, C: Candidate + Serialize + for<'a> Deserialize<'a>> Deserialize<'de> for Individual<C> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Inner<C> {
            cand: serde_json::Value,
            score: Score,
            #[serde(skip)]
            _c: std::marker::PhantomData<C>,
        }
        let v = Inner::<C>::deserialize(d)?;
        let cand: C = serde_json::from_value(v.cand).map_err(serde::de::Error::custom)?;
        Ok(Individual {
            cand,
            score: v.score,
            crowding_distance: 0.0,
        })
    }
}

pub struct EvaluationCache {
    cache: RwLock<HashMap<u64, Score>>,
}

impl EvaluationCache {
    pub fn new() -> Self {
        EvaluationCache {
            cache: RwLock::new(HashMap::new()),
        }
    }
    pub fn get(&self, id: u64) -> Option<Score> {
        self.cache.read().unwrap().get(&id).cloned()
    }
    pub fn insert(&self, id: u64, score: Score) {
        self.cache.write().unwrap().insert(id, score);
    }
}

pub struct Checkpoint<C: Candidate> {
    pub generation: u64,
    pub config: Config,
    pub archive: Vec<Individual<C>>,
}

impl<C: Candidate + Serialize + for<'a> Deserialize<'a>> Serialize for Checkpoint<C> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("Checkpoint", 3)?;
        st.serialize_field("generation", &self.generation)?;
        st.serialize_field("config", &self.config)?;
        st.serialize_field("archive", &self.archive)?;
        st.end()
    }
}

impl<'de, C: Candidate + Serialize + for<'a> Deserialize<'a>> Deserialize<'de> for Checkpoint<C> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Inner<C> {
            generation: u64,
            config: Config,
            archive: Vec<serde_json::Value>,
            #[serde(skip)]
            _c: std::marker::PhantomData<C>,
        }
        let v = Inner::<C>::deserialize(d)?;
        let mut archive = Vec::with_capacity(v.archive.len());
        for jv in v.archive {
            let i: Individual<C> = serde_json::from_value(jv).map_err(serde::de::Error::custom)?;
            archive.push(i);
        }
        Ok(Checkpoint {
            generation: v.generation,
            config: v.config,
            archive,
        })
    }
}

impl<C: Candidate + Serialize + for<'a> Deserialize<'a>> Checkpoint<C> {
    pub fn save_atomic(&self, path: &Path) -> std::io::Result<()> {
        let tmp_path = path.with_extension("tmp");
        let mut file = File::create(&tmp_path)?;
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
        fs::rename(tmp_path, path)?;
        Ok(())
    }

    pub fn load(path: &Path) -> std::io::Result<Self> {
        let data = fs::read_to_string(path)?;
        serde_json::from_str(&data).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}

pub struct Report<C: Candidate> {
    pub best: Option<Individual<C>>,
    pub final_baseline: Option<Score>,
    pub holdout_best: Option<Score>,
    pub holdout_baseline: Option<Score>,
    pub history: Vec<f64>,
}

pub struct Engine<D: Domain> {
    domain: D,
    config: Config,
    cache: EvaluationCache,
}

impl<D: Domain> Engine<D>
where
    D::Cand: Serialize + for<'a> Deserialize<'a>,
{
    pub fn new(domain: D, config: Config) -> Self {
        Engine {
            domain,
            config,
            cache: EvaluationCache::new(),
        }
    }

    pub fn run(&self) -> Result<Report<D::Cand>> {
        let mut master = StdRng::seed_from_u64(self.config.base_seed);
        let mut archive: Vec<Individual<D::Cand>> = Vec::new();
        let mut history: Vec<f64> = Vec::with_capacity(self.config.generations as usize);

        // ── Warm-start : amorçage avec candidats pré-existants, puis seed. ──
        let mut pop: Vec<D::Cand> = self.domain.warm_start();
        while pop.len() < self.config.population {
            pop.push(self.domain.seed(&mut master));
        }

        let mut final_baseline: Option<Score> = None;
        let mut generations_without_progress = 0u64;
        let mut last_best_score = f64::INFINITY;

        for g in 0..self.config.generations {
            let trial = Trial {
                generation: g,
                seed: self
                    .config
                    .base_seed
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    .wrapping_add(g.wrapping_add(1)),
            };

            // Stagnation détectée ? Hyper-mutation adaptative.
            let is_stagnating = generations_without_progress >= self.config.stagnation_threshold;

            // ── Évaluation parallèle + cache thread-safe + reflection. ──
            let evaluated_results: Result<Vec<Individual<D::Cand>>> = pop
                .par_iter()
                .map(|cand| {
                    let cand_id = cand.id();
                    if let Some(cached_score) = self.cache.get(cand_id) {
                        return Ok(Individual {
                            cand: cand.clone(),
                            score: cached_score,
                            crowding_distance: 0.0,
                        });
                    }

                    let current_cand = cand.clone();
                    let score = evaluate_with_reflection(
                        &self.domain,
                        &current_cand,
                        &trial,
                        self.config.max_reflection_retries,
                    )?;
                    self.cache.insert(cand_id, score.clone());

                    Ok(Individual {
                        cand: current_cand,
                        score,
                        crowding_distance: 0.0,
                    })
                })
                .collect();

            let mut valids: Vec<Individual<D::Cand>> = evaluated_results?
                .into_iter()
                .filter(|ind| ind.score.valid)
                .collect();

            final_baseline = Some(self.domain.baseline(&trial)?);

            valids.sort_by(|a, b| {
                a.score.objectives[0]
                    .partial_cmp(&b.score.objectives[0])
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let current_best = valids
                .first()
                .map(|i| i.score.objectives[0])
                .unwrap_or(f64::INFINITY);
            history.push(current_best);

            if current_best < last_best_score - 1e-6 {
                last_best_score = current_best;
                generations_without_progress = 0;
            } else {
                generations_without_progress += 1;
            }

            archive.append(&mut valids);
            let mut seen = HashSet::new();
            archive.retain(|i| seen.insert(i.cand.id()));

            // ── NSGA-II : fronts de non-domination + crowding distance. ──
            nsga2_select(&mut archive, self.config.survivors);

            let cp = Checkpoint {
                generation: g,
                config: self.config,
                archive: archive.clone(),
            };
            let _ = cp.save_atomic(Path::new("forge_checkpoint.json"));

            let survivors: Vec<D::Cand> = archive.iter().map(|i| i.cand.clone()).collect();
            if survivors.is_empty() {
                pop = (0..self.config.population)
                    .map(|_| self.domain.seed(&mut master))
                    .collect();
                continue;
            }

            let mut next: Vec<D::Cand> = survivors.clone();
            while next.len() < self.config.population {
                let parent = &survivors[master.gen_range(0..survivors.len())];
                if is_stagnating {
                    // Hyper-mutation : 3 passes en cascade (1 + 2 supplémentaires).
                    let mut mutated = self.domain.mutate(&mut master, &[parent])?;
                    for _ in 0..2 {
                        mutated = self.domain.mutate(&mut master, &[&mutated])?;
                    }
                    next.push(mutated);
                } else {
                    next.push(self.domain.mutate(&mut master, &[parent])?);
                }
            }
            pop = next;
        }

        let best = archive.into_iter().next();
        let (holdout_best, holdout_baseline) = match &best {
            Some(b) => {
                let holdout = Trial {
                    generation: u64::MAX,
                    seed: self.config.base_seed ^ 0xDEAD_BEEF_CAFE_F00D,
                };
                (
                    Some(evaluate_raw(&self.domain, &b.cand, &holdout)?),
                    Some(self.domain.baseline(&holdout)?),
                )
            }
            None => (None, None),
        };

        Ok(Report {
            best,
            final_baseline,
            holdout_best,
            holdout_baseline,
            history,
        })
    }
}

// ── Boucle d'auto-correction (self-reflection). ─────────────────────────────
//
// Tant que `evaluate_raw` ne renvoie pas un score valide ET qu'on n'a pas
// dépassé `max_retries`, on demande au domaine une version corrigée via
// `domain.reflect()`. Si le domaine ne sait pas corriger (renvoie None),
// on abandonne avec un score invalide.
fn evaluate_with_reflection<D: Domain>(
    domain: &D,
    cand: &D::Cand,
    trial: &Trial,
    max_retries: usize,
) -> Result<Score> {
    let mut current_cand = cand.clone();
    let mut rng = trial.rng();

    for attempt in 0..=max_retries {
        let res = evaluate_raw(domain, &current_cand, trial)?;
        if res.valid {
            return Ok(res);
        }
        if attempt < max_retries {
            if let Some(err_msg) = &res.error_msg {
                if let Some(corrected) = domain.reflect(&mut rng, &current_cand, err_msg)? {
                    current_cand = corrected;
                    continue;
                }
            }
        }
    }
    Ok(Score::invalid())
}

// ── Évaluation brute : verify (porte de correction) puis measure. ──────────
//
// Les erreurs `ForgeError::Evaluation` (issues de la compilation ou du
// runtime du candidat) sont capturées dans `Score::error_msg` pour boucler
// avec `reflect()`. Les autres erreurs (Generation, InvalidCandidate) sont
// propagées.
fn evaluate_raw<D: Domain>(domain: &D, cand: &D::Cand, trial: &Trial) -> Result<Score> {
    match domain.verify(cand, trial) {
        Ok(true) => {
            let objectives = domain.measure(cand, trial)?;
            if objectives.is_empty() || !objectives.iter().all(|x| x.is_finite()) {
                Ok(Score::invalid())
            } else {
                Ok(Score::valid(objectives))
            }
        }
        Ok(false) => Ok(Score::invalid()),
        Err(ForgeError::Evaluation(msg)) => Ok(Score::with_error(msg)),
        Err(e) => Err(e),
    }
}

// ── Sélection NSGA-II : fronts de non-domination + crowding distance. ──────
//
// Étapes :
//  1. Découper l'archive en fronts de Pareto (rang 0 = non-dominé, etc.).
//  2. Pour chaque front de taille > 2 : calculer la crowding distance
//     objectif par objectif (les bornes du front reçoivent +inf).
//  3. Réinjecter en respectant l'ordre rang ascendant puis crowding
//     distance décroissante, et tronquer à `survivors_limit`.
//
// Invariants respectés :
//  - Crowding distance sérialisée ignorée (#[serde(skip)] ci-dessus).
//  - Zéro allocation inutile hors les Vecs temporaires.
fn nsga2_select<C: Candidate>(archive: &mut Vec<Individual<C>>, survivors_limit: usize) {
    let total_len = archive.len();
    if total_len <= survivors_limit {
        return;
    }

    // 1. Découpage en fronts de non-domination.
    let mut ranks: Vec<Vec<usize>> = Vec::new();
    let mut remaining_indices: HashSet<usize> = (0..total_len).collect();

    while !remaining_indices.is_empty() {
        let mut current_front = Vec::new();
        for &i in &remaining_indices {
            let mut is_dominated = false;
            for &j in &remaining_indices {
                if i != j && archive[j].score.dominates(&archive[i].score) {
                    is_dominated = true;
                    break;
                }
            }
            if !is_dominated {
                current_front.push(i);
            }
        }
        if current_front.is_empty() {
            break;
        }
        for &idx in &current_front {
            remaining_indices.remove(&idx);
        }
        ranks.push(current_front);
    }

    // 2. Crowding distance par front + 3. Réinjection ordonnée.
    let mut final_selection: Vec<Individual<C>> = Vec::with_capacity(total_len);
    let num_objectives = archive[0].score.objectives.len();

    for front in &mut ranks {
        if front.is_empty() {
            continue;
        }
        let front_len = front.len();

        if front_len <= 2 {
            // Pas de voisinage exploitable : tout le front reçoit +inf pour
            // qu'aucun ne soit sacrifié par la crowding distance.
            for &idx in front.iter() {
                archive[idx].crowding_distance = f64::INFINITY;
            }
        } else {
            for &idx in front.iter() {
                archive[idx].crowding_distance = 0.0;
            }
            for obj_idx in 0..num_objectives {
                front.sort_by(|&a, &b| {
                    archive[a].score.objectives[obj_idx]
                        .partial_cmp(&archive[b].score.objectives[obj_idx])
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                archive[front[0]].crowding_distance = f64::INFINITY;
                archive[front[front_len - 1]].crowding_distance = f64::INFINITY;

                let min_val = archive[front[0]].score.objectives[obj_idx];
                let max_val = archive[front[front_len - 1]].score.objectives[obj_idx];
                let norm = max_val - min_val;

                if norm > 1e-9 {
                    for k in 1..(front_len - 1) {
                        let prev = front[k - 1];
                        let next = front[k + 1];
                        let curr = front[k];
                        if archive[curr].crowding_distance != f64::INFINITY {
                            archive[curr].crowding_distance += (archive[next].score.objectives
                                [obj_idx]
                                - archive[prev].score.objectives[obj_idx])
                                / norm;
                        }
                    }
                }
            }
        }
        for &idx in front.iter() {
            final_selection.push(archive[idx].clone());
        }
    }

    // Tri final : rang ascendant, crowding distance décroissante.
    // L'enumerate() capture l'ordre d'insertion (rang 0 d'abord, etc.).
    let mut annotated: Vec<(Individual<C>, usize)> = final_selection
        .into_iter()
        .enumerate()
        .map(|(order, ind)| (ind, order))
        .collect();
    annotated.sort_by(|a, b| {
        let front_a = ranks
            .iter()
            .position(|f| f.contains(&a.1))
            .unwrap_or(usize::MAX);
        let front_b = ranks
            .iter()
            .position(|f| f.contains(&b.1))
            .unwrap_or(usize::MAX);
        front_a.cmp(&front_b).then_with(|| {
            b.0.crowding_distance
                .partial_cmp(&a.0.crowding_distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });

    *archive = annotated.into_iter().map(|(ind, _)| ind).collect();
    archive.truncate(survivors_limit);
}
