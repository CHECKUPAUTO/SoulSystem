//! Boucle évolutionnaire générique industrielle. Le moteur orchestre de manière
//! parallélisée les étapes d'évaluation (porte de correction puis mesure) tout en
//! protégeant le framework contre le sur-apprentissage grâce à des graines rotatives.
//! L'archive d'élites utilise un tri de non-domination de Pareto multi-objectif.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rayon::prelude::*;
use serde::{Serialize, Deserialize};

use crate::candidate::Candidate;
use crate::domain::{Domain, Score};
use crate::error::Result;
use crate::trial::Trial;

/// Paramètres de la recherche.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Config {
    /// Nombre de générations.
    pub generations: u64,
    /// Taille de la population à chaque génération.
    pub population: usize,
    /// Nombre d'élites conservés (et reproducteurs) entre générations.
    pub survivors: usize,
    /// Graine maîtresse (rend toute la campagne reproductible).
    pub base_seed: u64,
}

/// Un individu évalué.
#[derive(Clone)]
pub struct Individual<C: Candidate> {
    pub cand: C,
    pub score: Score,
}

impl<C: Candidate + Serialize + for<'a> Deserialize<'a>> Serialize for Individual<C> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("Individual", 2)?;
        st.serialize_field("cand", &self.cand)?;
        st.serialize_field("score", &self.score)?;
        st.end()
    }
}

impl<'de, C: Candidate + Serialize + for<'a> Deserialize<'a>> Deserialize<'de> for Individual<C> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct Inner<C: crate::candidate::Candidate> { cand: C, score: Score }
        impl<'de, C> serde::Deserialize<'de> for Inner<C>
        where C: crate::candidate::Candidate + serde::Serialize + for<'a> serde::Deserialize<'a>,
        {
            fn deserialize<D: serde::Deserializer<'de>>(__deserializer: D) -> std::result::Result<Self, D::Error> {
                #[derive(serde::Deserialize)]
                struct __SerdeVisit<C> { cand: serde_json::Value, score: Score, _c: std::marker::PhantomData<C> }
                let v = __SerdeVisit::<C>::deserialize(__deserializer)?;
                let cand: C = serde_json::from_value(v.cand).map_err(serde::de::Error::custom)?;
                Ok(Inner { cand, score: v.score })
            }
        }
        let inner = Inner::deserialize(d)?;
        Ok(Individual { cand: inner.cand, score: inner.score })
    }
}

/// Structure de persistance pour sauvegarder/charger l'état de l'archive.
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
        struct __CheckpointInner<C: crate::candidate::Candidate> { generation: u64, config: Config, archive: Vec<Individual<C>> }
        impl<'de, C> serde::Deserialize<'de> for __CheckpointInner<C>
        where C: crate::candidate::Candidate + serde::Serialize + for<'a> serde::Deserialize<'a>,
        {
            fn deserialize<D: serde::Deserializer<'de>>(__deserializer: D) -> std::result::Result<Self, D::Error> {
                #[derive(serde::Deserialize)]
                struct __V<C> { generation: u64, config: Config, archive: Vec<serde_json::Value>, _c: std::marker::PhantomData<C> }
                let v = __V::<C>::deserialize(__deserializer)?;
                let mut archive = Vec::with_capacity(v.archive.len());
                for ind in v.archive {
                    let i: Individual<C> = serde_json::from_value(ind).map_err(serde::de::Error::custom)?;
                    archive.push(i);
                }
                Ok(__CheckpointInner { generation: v.generation, config: v.config, archive })
            }
        }
        let inner: __CheckpointInner<C> = __CheckpointInner::deserialize(d)?;
        Ok(Checkpoint { generation: inner.generation, config: inner.config, archive: inner.archive })
    }
}

impl<C: Candidate + Serialize + for<'a> Deserialize<'a>> Checkpoint<C> {
    /// Sauvegarde de l'archive de manière atomique (écriture temporaire puis renommage POSIX).
    pub fn save_atomic(&self, path: &Path) -> std::io::Result<()> {
        let tmp_path = path.with_extension("tmp");
        let mut file = File::create(&tmp_path)?;
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        file.write_all(json.as_bytes())?;
        file.sync_all()?; // Force le flush sur le support physique (NVMe)
        fs::rename(tmp_path, path)?;
        Ok(())
    }

    /// Charge un checkpoint sauvegardé.
    #[allow(dead_code)]
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let data = fs::read_to_string(path)?;
        serde_json::from_str(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}

/// Compte-rendu de campagne.
pub struct Report<C: Candidate> {
    /// Meilleur candidat archivé (objectif principal minimal).
    pub best: Option<Individual<C>>,
    /// Baseline mesurée au dernier essai d'evolution.
    pub final_baseline: Option<Score>,
    /// Score du meilleur candidat sur le holdout (graine inédite).
    pub holdout_best: Option<Score>,
    /// Baseline sur le holdout, pour comparaison équitable.
    pub holdout_baseline: Option<Score>,
    /// Meilleur objectif principal par génération (courbe d'apprentissage).
    pub history: Vec<f64>,
}

/// Le moteur, paramétré par un domaine.
pub struct Engine<D: Domain> {
    domain: D,
    config: Config,
}

impl<D: Domain> Engine<D>
where
    D::Cand: Serialize + for<'a> Deserialize<'a>,
{
    pub fn new(domain: D, config: Config) -> Self {
        Engine { domain, config }
    }

    /// Lance la campagne complète avec parallélisation native et tri de Pareto.
    pub fn run(&self) -> Result<Report<D::Cand>> {
        let mut master = StdRng::seed_from_u64(self.config.base_seed);

        // Population initiale.
        let mut pop: Vec<D::Cand> = (0..self.config.population)
            .map(|_| self.domain.seed(&mut master))
            .collect();

        let mut archive: Vec<Individual<D::Cand>> = Vec::new();
        let mut history: Vec<f64> = Vec::with_capacity(self.config.generations as usize);
        let mut final_baseline: Option<Score> = None;

        for g in 0..self.config.generations {
            // Graine d'essai tournante (anti-overfit).
            let trial = Trial {
                generation: g,
                seed: self
                    .config
                    .base_seed
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    .wrapping_add(g.wrapping_add(1)),
            };

            // ÉVALUATION PARALLÈLE : Exploite l'ensemble des cœurs système disponibles via Rayon
            let evaluated_results: Result<Vec<Individual<D::Cand>>> = pop
                .par_iter()
                .map(|cand| {
                    let score = evaluate(&self.domain, cand, &trial)?;
                    Ok(Individual { cand: cand.clone(), score })
                })
                .collect();

            // Extraction des individus valides
            let mut valids: Vec<Individual<D::Cand>> = evaluated_results?
                .into_iter()
                .filter(|ind| ind.score.valid)
                .collect();

            final_baseline = Some(self.domain.baseline(&trial)?);

            // Suivi de l'historique sur l'objectif principal de la génération actuelle
            valids.sort_by(|a, b| cmp_primary(&a.score, &b.score));
            history.push(valids.first().map(|i| i.score.objectives[0]).unwrap_or(f64::INFINITY));

            // Fusion et déduplication stricte par ID de contenu syntaxique
            archive.append(&mut valids);
            let mut seen: HashSet<u64> = HashSet::new();
            archive.retain(|i| seen.insert(i.cand.id()));

            // TRI MULTI-OBJECTIF : Sélection par couches de non-domination de Pareto
            sort_by_pareto_domination(&mut archive);
            archive.truncate(self.config.survivors.max(1));

            // Déclenchement facultatif d'une sauvegarde de sécurité à chaque génération
            let cp = Checkpoint {
                generation: g,
                config: self.config,
                archive: archive.clone(),
            };
            let _ = cp.save_atomic(Path::new("forge_checkpoint.json"));

            // Reproduction : Élitisme multi-objectif + mutation
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
                next.push(self.domain.mutate(&mut master, &[parent])?);
            }
            pop = next;
        }

        // Validation finale stricte sur la graine isolée de holdout
        let best = archive.into_iter().next();
        let (holdout_best, holdout_baseline) = match &best {
            Some(b) => {
                let holdout = Trial {
                    generation: u64::MAX,
                    seed: self.config.base_seed ^ 0xDEAD_BEEF_CAFE_F00D,
                };
                (
                    Some(evaluate(&self.domain, &b.cand, &holdout)?),
                    Some(self.domain.baseline(&holdout)?),
                )
            }
            None => (None, None),
        };

        Ok(Report { best, final_baseline, holdout_best, holdout_baseline, history })
    }
}

/// Harnais d'évaluation unifié.
fn evaluate<D: Domain>(domain: &D, cand: &D::Cand, trial: &Trial) -> Result<Score> {
    if !domain.verify(cand, trial)? {
        return Ok(Score::invalid());
    }
    let objectives = domain.measure(cand, trial)?;
    if objectives.is_empty() || !objectives.iter().all(|x| x.is_finite()) {
        return Ok(Score::invalid());
    }
    Ok(Score::valid(objectives))
}

/// Comparaison brute sur le premier objectif (minimisation).
fn cmp_primary(a: &Score, b: &Score) -> std::cmp::Ordering {
    let av = a.objectives.first().copied().unwrap_or(f64::INFINITY);
    let bv = b.objectives.first().copied().unwrap_or(f64::INFINITY);
    av.partial_cmp(&bv).unwrap_or(std::cmp::Ordering::Equal)
}

/// Trie l'archive en plaçant d'abord les individus appartenant aux meilleurs fronts
/// de Pareto (ceux qui sont le moins dominés par les autres). Rompt les égalités
/// en triant selon l'objectif principal de performance.
fn sort_by_pareto_domination<C: Candidate>(archive: &mut Vec<Individual<C>>) {
    let len = archive.len();
    if len <= 1 { return; }

    // Calcule pour chaque individu le nombre de fois qu'il se fait dominer par un autre membre
    let mut domination_counts = vec![0usize; len];
    for i in 0..len {
        for j in 0..len {
            if i != j && archive[j].score.dominates(&archive[i].score) {
                domination_counts[i] += 1;
            }
        }
    }

    // On associe temporairement les scores de domination pour trier sans recalculer
    let mut annotated: Vec<(Individual<C>, usize)> = archive
        .drain(..)
        .zip(domination_counts.into_iter())
        .collect();

    annotated.sort_by(|a, b| {
        // Priorité absolue au plus petit nombre de dominations (Fronts NSGA-II simplifiés)
        a.1.cmp(&b.1).then_with(|| cmp_primary(&a.0.score, &b.0.score))
    });

    // Réinjection dans le vecteur d'origine trié
    archive.extend(annotated.into_iter().map(|(ind, _)| ind));
}
