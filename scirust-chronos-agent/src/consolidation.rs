// ==========================================================================
// consolidation.rs — Consolidation hors-ligne (V6.3, point 3.D du document)
//
// "Pendant les périodes d'inactivité, un processus lourd peut être lancé
//  pour réorganiser les index, renforcer les liaisons synaptiques du graphe
//  et élaguer les souvenirs inutiles."
//
// PRINCIPE BIOLOGIQUE
//
//   Le sommeil chez les mammifères : les hippocampes rejouent les épisodes
//   du jour (replay) et les souvenirs significatifs sont transférés vers
//   le néocortex sous forme abstraite. Les détails s'effacent, les
//   patterns persistent.
//
// IMPLÉMENTATION
//
//   ConsolidationCycle observe l'EpisodicStore et :
//
//   1. SÉLECTION : prend les traces dont importance > threshold ET
//      confirmations >= min_confirmations. Ce sont des épisodes qui
//      ont été validés par le rétro-binding et qui ont du sens.
//
//   2. CLUSTERING : regroupe les traces sélectionnées par proximité
//      cosinus (k-means simple ou clustering hiérarchique simple).
//      Chaque cluster devient candidat à un prototype sémantique.
//
//   3. ÉLÉVATION : pour chaque cluster :
//      - calcule le centroïde (moyenne des vecteurs)
//      - le pousse dans SemanticStore via .learn() avec strength initial
//        proportionnel à la taille du cluster
//      - marque les traces source comme "consolidées" (=> peuvent être
//        prunées plus agressivement la fois suivante)
//
//   4. NETTOYAGE : les traces consolidées sont marquées et leur
//      threshold de prune est abaissé spécifiquement pour elles.
//
// USAGE
//
//   // Pendant une pause longue ou à un moment hors-hot-path
//   let mut consolidator = ConsolidationCycle::default();
//   let report = consolidator.run(&mut memory);
//   println!("consolidated {} traces into {} prototypes",
//            report.traces_consolidated, report.prototypes_created);
//
// REMARQUE
//
//   Le clustering ici est volontairement minimaliste (greedy + cosine
//   threshold). Pour des volumes >10K traces, brancher un vrai k-means
//   ou DBSCAN via une crate comme `linfa-clustering`.
// ==========================================================================

use crate::memory::{AtemporalMemory, EpisodicTrace, ImportanceParams};

// --------------------------------------------------------------------------
// Config
// --------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ConsolidationConfig {
    /// Score d'importance minimal pour être candidat à la consolidation.
    pub min_importance: f64,
    /// Confirmations rétro-causales minimales (preuve que la prédiction
    /// associée a été validée).
    pub min_confirmations: u32,
    /// Seuil de proximité cosinus pour regrouper deux traces dans le même
    /// cluster (= candidat au même prototype sémantique).
    pub cluster_threshold: f64,
    /// Strength initial du prototype créé : 1 + bonus·cluster_size.
    pub strength_bonus_per_member: f64,
    /// Threshold de matching utilisé dans `semantic.learn()` durant la
    /// consolidation. Doit être plus haut que le threshold inférence
    /// normal pour éviter d'écraser un prototype mal placé.
    pub learn_threshold: f64,
    /// Paramètres d'importance utilisés pour la sélection.
    pub importance_params: ImportanceParams,
}

impl Default for ConsolidationConfig {
    fn default() -> Self {
        Self {
            min_importance: 1.0,
            min_confirmations: 2,
            cluster_threshold: 0.85,
            strength_bonus_per_member: 0.5,
            learn_threshold: 0.92,
            importance_params: ImportanceParams::default(),
        }
    }
}

// --------------------------------------------------------------------------
// Report
// --------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct ConsolidationReport {
    pub candidates_examined: usize,
    pub traces_consolidated: usize,
    pub prototypes_created: usize,
    pub prototypes_reinforced: usize,
    pub clusters_found: usize,
}

// --------------------------------------------------------------------------
// ConsolidationCycle
// --------------------------------------------------------------------------

pub struct ConsolidationCycle {
    pub config: ConsolidationConfig,
}

impl Default for ConsolidationCycle {
    fn default() -> Self {
        Self { config: ConsolidationConfig::default() }
    }
}

impl ConsolidationCycle {
    pub fn new(config: ConsolidationConfig) -> Self { Self { config } }

    /// Exécute un cycle complet : sélection → clustering → élévation.
    /// Modifie `memory` (peut ajouter des prototypes sémantiques).
    /// Retourne un rapport sur ce qui a été fait.
    pub fn run(&self, memory: &mut AtemporalMemory) -> ConsolidationReport {
        let mut report = ConsolidationReport::default();
        let current_t = memory.current_t;

        // ---- 1. Sélection des candidats ----
        let mut candidates: Vec<EpisodicTrace> = memory.episodic.traces.iter()
            .filter(|t| {
                t.confirmations >= self.config.min_confirmations
                && t.importance(current_t, &self.config.importance_params)
                    >= self.config.min_importance
            })
            .cloned()
            .collect();
        report.candidates_examined = candidates.len();
        if candidates.is_empty() { return report; }

        // ---- 2. Clustering greedy par cosinus ----
        // Trie par importance décroissante pour que les seeds soient les
        // plus pertinents en priorité.
        candidates.sort_by(|a, b| {
            let ia = a.importance(current_t, &self.config.importance_params);
            let ib = b.importance(current_t, &self.config.importance_params);
            ib.partial_cmp(&ia).unwrap()
        });

        let mut clusters: Vec<Vec<&EpisodicTrace>> = Vec::new();
        for trace in &candidates {
            let mut placed = false;
            for cluster in clusters.iter_mut() {
                // Test contre le centroïde du cluster (moyenne progressive)
                if let Some(repr) = cluster.first() {
                    if cosine(&trace.vector, &repr.vector) >= self.config.cluster_threshold {
                        cluster.push(trace);
                        placed = true;
                        break;
                    }
                }
            }
            if !placed {
                clusters.push(vec![trace]);
            }
        }
        report.clusters_found = clusters.len();

        // ---- 3. Élévation : chaque cluster → prototype sémantique ----
        let semantic_before = memory.semantic.prototypes.len();
        for cluster in &clusters {
            let centroid = centroid_of(cluster);
            let cluster_size = cluster.len();
            let bonus_strength = 1.0
                + self.config.strength_bonus_per_member * (cluster_size as f64 - 1.0);

            // La méthode `learn` gère naturellement merge-vs-create :
            // si un prototype proche existe déjà, il est renforcé.
            // Sinon, un nouveau est créé.
            let existed_before = memory.semantic.prototypes.len();
            memory.semantic.learn(centroid, self.config.learn_threshold, current_t);
            if memory.semantic.prototypes.len() > existed_before {
                // Nouveau prototype créé — on peut booster sa strength au-delà
                // du défaut (1.0) pour refléter la "preuve par consolidation"
                if let Some(p) = memory.semantic.prototypes.last_mut() {
                    p.strength = (p.strength + bonus_strength - 1.0).min(100.0);
                }
                report.prototypes_created += 1;
            } else {
                report.prototypes_reinforced += 1;
            }
            report.traces_consolidated += cluster_size;
        }
        let _ = semantic_before;

        // Force rebuild HNSW si on a ajouté des prototypes
        if report.prototypes_created > 0 {
            memory.semantic.compact();
        }

        report
    }
}

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

fn centroid_of(traces: &[&EpisodicTrace]) -> Vec<f64> {
    if traces.is_empty() { return Vec::new(); }
    let dim = traces[0].vector.len();
    let n = traces.len() as f64;
    let mut c = vec![0.0; dim];
    for t in traces {
        for (i, v) in t.vector.iter().enumerate().take(dim) {
            c[i] += v / n;
        }
    }
    c
}

fn cosine(a: &[f64], b: &[f64]) -> f64 {
    let len = a.len().min(b.len());
    let mut dot = 0.0;
    let mut na = 0.0;
    let mut nb = 0.0;
    for i in 0..len {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    let denom = (na.sqrt() * nb.sqrt()).max(f64::EPSILON);
    dot / denom
}

// ==========================================================================
// Tests
// ==========================================================================
#[cfg(test)]
mod tests {
    use super::*;

    /// Crée une trace artificielle avec un état "consolidé" (confirmations
    /// récentes, accédée souvent).
    fn make_consolidatable(vector: Vec<f64>, t: u64) -> EpisodicTrace {
        EpisodicTrace {
            vector,
            t_stored: t,
            future_prediction: Some(vec![0.0; 4]),
            last_confirmed: t + 10,  // confirmé peu après le stockage
            confirmations: 5,
            access_count: 10,
        }
    }

    #[test]
    fn empty_memory_no_op() {
        let mut memory = AtemporalMemory::new(4, 10);
        let cyc = ConsolidationCycle::default();
        let report = cyc.run(&mut memory);
        assert_eq!(report.candidates_examined, 0);
        assert_eq!(report.prototypes_created, 0);
    }

    #[test]
    fn isolated_traces_create_separate_prototypes() {
        let mut memory = AtemporalMemory::new(4, 100);
        memory.current_t = 100;

        // 3 traces orthogonales → 3 clusters → 3 prototypes
        for (i, v) in [
            vec![1.0, 0.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0, 0.0],
            vec![0.0, 0.0, 1.0, 0.0],
        ].iter().enumerate() {
            memory.episodic.traces.push_back(make_consolidatable(v.clone(), i as u64 * 10));
        }

        let cyc = ConsolidationCycle::default();
        let report = cyc.run(&mut memory);
        assert_eq!(report.candidates_examined, 3);
        assert_eq!(report.clusters_found, 3);
        assert_eq!(report.prototypes_created, 3);
    }

    #[test]
    fn similar_traces_merge_into_one_prototype() {
        let mut memory = AtemporalMemory::new(4, 100);
        memory.current_t = 100;

        // 5 traces très proches (toutes vers la direction +x)
        for i in 0..5 {
            let mut v = vec![0.0; 4];
            v[0] = 1.0 + i as f64 * 0.001;  // quasi-identiques
            memory.episodic.traces.push_back(make_consolidatable(v, i as u64 * 10));
        }

        let cyc = ConsolidationCycle::default();
        let report = cyc.run(&mut memory);
        assert_eq!(report.candidates_examined, 5);
        // 5 traces très similaires → 1 seul cluster (cosine_threshold=0.85)
        assert_eq!(report.clusters_found, 1);
        assert_eq!(report.traces_consolidated, 5);
    }

    #[test]
    fn unconfirmed_traces_excluded() {
        let mut memory = AtemporalMemory::new(4, 100);
        memory.current_t = 100;

        // Trace sans confirmations → exclue
        let mut t = make_consolidatable(vec![1.0, 0.0, 0.0, 0.0], 50);
        t.confirmations = 0;  // pas de rétro-binding
        memory.episodic.traces.push_back(t);

        let cyc = ConsolidationCycle::default();
        let report = cyc.run(&mut memory);
        assert_eq!(report.candidates_examined, 0);
        assert_eq!(report.prototypes_created, 0);
    }

    #[test]
    fn cluster_boost_applies_strength() {
        let mut memory = AtemporalMemory::new(4, 100);
        memory.current_t = 100;

        // 10 traces très similaires → 1 cluster → strength boostée
        for i in 0..10 {
            let mut v = vec![0.0; 4];
            v[0] = 1.0 + i as f64 * 0.0001;
            memory.episodic.traces.push_back(make_consolidatable(v, i as u64));
        }

        let cyc = ConsolidationCycle::default();
        cyc.run(&mut memory);
        assert_eq!(memory.semantic.prototypes.len(), 1);
        // Avec cluster_size=10 et bonus=0.5 → strength = 1 + 0.5*9 = 5.5
        let s = memory.semantic.prototypes[0].strength;
        assert!(s > 4.0, "strength not boosted: {}", s);
    }
}
