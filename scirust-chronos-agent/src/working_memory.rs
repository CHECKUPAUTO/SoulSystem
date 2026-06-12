// ==========================================================================
// working_memory.rs — Mémoire de travail étendue (V6.4, point 2.C du doc)
//
// "Au lieu de laisser le contexte se saturer passivement, implémenter un
//  gestionnaire qui :
//   - attribue des priorités aux éléments chargés en fonction du focus courant
//   - décharge automatiquement les éléments non prioritaires vers un
//     buffer de rappel rapide
//   - réinjecte ces éléments si le contexte change"
//
// COMPLÉMENT À LA MÉMOIRE EPISODIQUE/SEMANTIQUE
//
//   La mémoire de travail est différente :
//   - Épisodique : trace brute d'observations, large mais accès O(log n)
//   - Sémantique : prototypes abstraits, condensés mais moins riches
//   - Working memory : "qu'est-ce qui est ACTIF maintenant"
//
//   C'est ce qui permet à l'agent de maintenir un FOCUS — savoir où il en
//   est dans la conversation/raisonnement sans devoir tout re-fetcher
//   depuis l'épisodique à chaque step.
//
// ARCHITECTURE
//
//   - SLOTS prioritisés : N slots fixes (par défaut 8), chacun contient
//     un item avec un score de priorité.
//   - HOT BUFFER : les items chargés, immédiatement accessibles.
//   - COLD BUFFER : items évincés mais "rappelables" rapidement (file
//     LIFO bornée, ne pollue pas la sémantique long-terme).
//   - FOCUS VECTOR : vecteur de contexte qui guide la priorisation —
//     mis à jour par l'agent à chaque tour, typiquement = état BCI ou
//     embedding du sujet courant.
//
// PRIORITISATION
//
//   priority(item) = α·cosine(item.vector, focus)
//                  + β·log(1 + access_count)
//                  + γ·recency_factor
//
//   À chaque update_focus(new_focus), tous les slots sont re-scorés.
//   Les items au-dessus de la médiane restent dans hot, les autres
//   migrent vers cold. Si focus change brutalement, les items pertinents
//   du cold sont re-promus.
// ==========================================================================

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

// --------------------------------------------------------------------------
// Item de mémoire de travail
// --------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkingItem {
    /// Identifiant stable (e.g. hash du vecteur, ou ID externe).
    pub id: u64,
    /// Représentation vectorielle pour scoring de priorité.
    pub vector: Vec<f64>,
    /// Payload optionnel — peut être un index dans la mémoire épisodique
    /// ou une chaîne décrivant l'item, etc.
    pub label: String,
    /// Compteur d'accès pour boost de priorité.
    pub access_count: u32,
    /// Step où l'item a été ajouté (pour recency).
    pub added_at: u64,
    /// Step du dernier accès.
    pub last_touched: u64,
    /// Score de priorité courant (calculé, pas stocké de manière permanente).
    pub priority: f64,
}

impl WorkingItem {
    pub fn touch(&mut self, current_t: u64) {
        self.access_count = self.access_count.saturating_add(1);
        self.last_touched = current_t;
    }
}

// --------------------------------------------------------------------------
// Configuration
// --------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct WorkingMemoryConfig {
    /// Nombre de slots dans le hot buffer (mémoire active).
    pub hot_capacity: usize,
    /// Taille max du cold buffer (rappel rapide).
    pub cold_capacity: usize,
    /// Pondération : similarité au focus.
    pub w_focus: f64,
    /// Pondération : fréquence d'accès.
    pub w_freq: f64,
    /// Pondération : recence.
    pub w_recency: f64,
    /// Constante de temps pour le decay de recency.
    pub tau_recency: f64,
}

impl Default for WorkingMemoryConfig {
    fn default() -> Self {
        Self {
            hot_capacity: 8,
            cold_capacity: 32,
            w_focus: 1.0,
            w_freq: 0.3,
            w_recency: 0.5,
            tau_recency: 50.0,
        }
    }
}

// --------------------------------------------------------------------------
// WorkingMemory
// --------------------------------------------------------------------------

pub struct WorkingMemory {
    pub config: WorkingMemoryConfig,
    /// Slots actifs, triés par priorité décroissante.
    pub hot: Vec<WorkingItem>,
    /// Buffer LIFO d'items évincés, candidat à réinjection.
    pub cold: VecDeque<WorkingItem>,
    /// Vecteur de focus courant — guide la priorisation.
    pub focus: Option<Vec<f64>>,
    /// Stats
    pub total_inserts: u64,
    pub total_evictions: u64,
    pub total_promotions: u64,
}

impl WorkingMemory {
    pub fn new() -> Self { Self::with_config(WorkingMemoryConfig::default()) }

    pub fn with_config(config: WorkingMemoryConfig) -> Self {
        Self {
            hot: Vec::with_capacity(config.hot_capacity),
            cold: VecDeque::with_capacity(config.cold_capacity),
            config,
            focus: None,
            total_inserts: 0,
            total_evictions: 0,
            total_promotions: 0,
        }
    }

    /// Met à jour le vecteur de focus et re-score tous les items.
    /// Les items qui dégringolent passent en cold, ceux du cold remontent
    /// éventuellement en hot.
    pub fn update_focus(&mut self, focus: Vec<f64>, current_t: u64) {
        self.focus = Some(focus);
        self.rescore(current_t);
        self.rebalance(current_t);
    }

    /// Ajoute un nouvel item ou met à jour s'il existe déjà (par id).
    pub fn insert(&mut self, item: WorkingItem, current_t: u64) {
        // Si l'id existe déjà en hot, touch et update
        for existing in self.hot.iter_mut() {
            if existing.id == item.id {
                existing.touch(current_t);
                // Garde le vecteur le plus récent (peut avoir évolué)
                existing.vector = item.vector;
                self.rescore(current_t);
                self.rebalance(current_t);
                return;
            }
        }
        // S'il est en cold, le promouvoir
        if let Some(pos) = self.cold.iter().position(|c| c.id == item.id) {
            let mut cold_item = self.cold.remove(pos).unwrap();
            cold_item.touch(current_t);
            cold_item.vector = item.vector;
            self.hot.push(cold_item);
            self.total_promotions += 1;
        } else {
            // Vraiment nouveau
            let mut new_item = item;
            new_item.added_at = current_t;
            new_item.last_touched = current_t;
            self.hot.push(new_item);
            self.total_inserts += 1;
        }
        self.rescore(current_t);
        self.rebalance(current_t);
    }

    /// Accès en lecture par id — incrémente access_count.
    pub fn touch(&mut self, id: u64, current_t: u64) -> Option<&WorkingItem> {
        if let Some(idx) = self.hot.iter().position(|i| i.id == id) {
            self.hot[idx].touch(current_t);
            return self.hot.get(idx);
        }
        None
    }

    /// Recherche du meilleur item proche d'un vecteur query.
    pub fn query(&self, query: &[f64]) -> Option<&WorkingItem> {
        let mut best: Option<(usize, f64)> = None;
        for (i, item) in self.hot.iter().enumerate() {
            let sim = cosine(query, &item.vector);
            if best.map(|(_, s)| sim > s).unwrap_or(true) {
                best = Some((i, sim));
            }
        }
        best.and_then(|(i, _)| self.hot.get(i))
    }

    /// Re-calcule les priorités à partir du focus courant.
    fn rescore(&mut self, current_t: u64) {
        let focus = self.focus.clone();
        let cfg = self.config.clone();
        for item in self.hot.iter_mut() {
            item.priority = score(item, focus.as_deref(), current_t, &cfg);
        }
        // Trie hot par priorité décroissante
        self.hot.sort_by(|a, b| b.priority.partial_cmp(&a.priority).unwrap());
    }

    /// Évince les items en dessous de la capacité vers cold ; promeut
    /// éventuellement des items du cold si la priorité courante les
    /// fait remonter.
    fn rebalance(&mut self, current_t: u64) {
        // 1. Si hot dépasse la capacité, déplace les surnuméraires (les
        //    moins prioritaires, en bas du Vec après tri) en cold.
        while self.hot.len() > self.config.hot_capacity {
            if let Some(evicted) = self.hot.pop() {
                self.push_to_cold(evicted);
            }
        }

        // 2. Si le focus a changé, certains items du cold peuvent maintenant
        //    avoir une priorité supérieure aux items moins prioritaires du hot.
        //    On compare le pire du hot vs le meilleur du cold.
        if let (Some(focus), false) = (self.focus.as_deref(), self.cold.is_empty()) {
            let cfg = &self.config;
            // Score tous les cold items (read-only)
            let mut cold_scores: Vec<(usize, f64)> = self.cold.iter()
                .enumerate()
                .map(|(i, c)| (i, score(c, Some(focus), current_t, cfg)))
                .collect();
            cold_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

            // Promouvoir tant qu'un cold > pire hot
            for (cold_idx, cold_score) in cold_scores {
                let worst_hot = self.hot.last().map(|h| h.priority).unwrap_or(f64::NEG_INFINITY);
                if cold_score > worst_hot && self.hot.len() == self.config.hot_capacity {
                    // Swap : évince worst hot, promeut cold
                    let promoted_idx = self.cold.iter().position(|c|
                        c.id == self.cold[cold_idx].id);
                    if let Some(pi) = promoted_idx {
                        let mut promoted = self.cold.remove(pi).unwrap();
                        promoted.priority = cold_score;
                        if let Some(evicted) = self.hot.pop() {
                            self.push_to_cold(evicted);
                        }
                        self.hot.push(promoted);
                        self.total_promotions += 1;
                    }
                } else {
                    break;
                }
            }
            // Re-trie hot après promotions
            self.hot.sort_by(|a, b| b.priority.partial_cmp(&a.priority).unwrap());
        }
    }

    fn push_to_cold(&mut self, item: WorkingItem) {
        while self.cold.len() >= self.config.cold_capacity {
            self.cold.pop_front();  // évince le plus ancien dans cold
        }
        self.cold.push_back(item);
        self.total_evictions += 1;
    }

    pub fn hot_len(&self) -> usize { self.hot.len() }
    pub fn cold_len(&self) -> usize { self.cold.len() }
    pub fn is_empty(&self) -> bool { self.hot.is_empty() && self.cold.is_empty() }

    pub fn clear(&mut self) {
        self.hot.clear();
        self.cold.clear();
        self.focus = None;
    }
}

impl Default for WorkingMemory {
    fn default() -> Self { Self::new() }
}

// --------------------------------------------------------------------------
// Scoring
// --------------------------------------------------------------------------

fn score(item: &WorkingItem, focus: Option<&[f64]>, current_t: u64,
         cfg: &WorkingMemoryConfig) -> f64
{
    let focus_score = match focus {
        Some(f) => cosine(&item.vector, f).max(0.0),  // clamp neg
        None => 0.5,
    };
    let freq_score = (1.0 + item.access_count as f64).ln();
    let dt = current_t.saturating_sub(item.last_touched) as f64;
    let recency = (-dt / cfg.tau_recency).exp();

    cfg.w_focus * focus_score
    + cfg.w_freq * freq_score
    + cfg.w_recency * recency
}

#[inline]
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

    fn make(id: u64, vec: Vec<f64>, label: &str) -> WorkingItem {
        WorkingItem {
            id, vector: vec, label: label.into(),
            access_count: 0, added_at: 0, last_touched: 0, priority: 0.0,
        }
    }

    #[test]
    fn fills_hot_up_to_capacity() {
        let mut wm = WorkingMemory::with_config(WorkingMemoryConfig {
            hot_capacity: 3, cold_capacity: 8, ..Default::default()
        });
        wm.update_focus(vec![1.0, 0.0, 0.0, 0.0], 0);
        for i in 0..3 {
            wm.insert(make(i, vec![1.0, 0.0, 0.0, 0.0], "x"), 0);
        }
        assert_eq!(wm.hot_len(), 3);
        assert_eq!(wm.cold_len(), 0);
    }

    #[test]
    fn evicts_to_cold_when_over_capacity() {
        let mut wm = WorkingMemory::with_config(WorkingMemoryConfig {
            hot_capacity: 2, cold_capacity: 8, ..Default::default()
        });
        wm.update_focus(vec![1.0, 0.0, 0.0, 0.0], 0);

        // 3 items proches du focus → tous candidats hot, mais cap=2 → 1 évincé
        for i in 0..3 {
            wm.insert(make(i, vec![1.0, 0.0, 0.0, 0.0], "x"), i);
        }
        assert_eq!(wm.hot_len(), 2);
        assert!(wm.cold_len() >= 1);
        assert!(wm.total_evictions >= 1);
    }

    #[test]
    fn focus_shift_repromotes_relevant_cold_items() {
        let mut wm = WorkingMemory::with_config(WorkingMemoryConfig {
            hot_capacity: 2, cold_capacity: 8, ..Default::default()
        });

        // Focus initial vers +x : insère 2 items +x et 1 item +y
        wm.update_focus(vec![1.0, 0.0, 0.0, 0.0], 0);
        wm.insert(make(0, vec![1.0, 0.0, 0.0, 0.0], "x1"), 0);
        wm.insert(make(1, vec![1.0, 0.0, 0.0, 0.0], "x2"), 1);
        wm.insert(make(2, vec![0.0, 1.0, 0.0, 0.0], "y"), 2);
        // L'item y a une priorité basse pour focus +x → cold
        assert!(wm.cold.iter().any(|c| c.id == 2),
                "y item should be in cold: hot={:?} cold={:?}",
                wm.hot.iter().map(|h| h.id).collect::<Vec<_>>(),
                wm.cold.iter().map(|c| c.id).collect::<Vec<_>>());

        // Shift focus vers +y → l'item y devrait être promu en hot
        wm.update_focus(vec![0.0, 1.0, 0.0, 0.0], 10);
        let in_hot = wm.hot.iter().any(|h| h.id == 2);
        assert!(in_hot, "y item should be promoted after focus shift: hot={:?}",
                wm.hot.iter().map(|h| h.id).collect::<Vec<_>>());
        assert!(wm.total_promotions >= 1);
    }

    #[test]
    fn query_returns_closest_in_hot() {
        let mut wm = WorkingMemory::with_config(WorkingMemoryConfig {
            hot_capacity: 4, cold_capacity: 8, ..Default::default()
        });
        wm.update_focus(vec![1.0, 1.0, 1.0, 1.0], 0);
        wm.insert(make(0, vec![1.0, 0.0, 0.0, 0.0], "x"), 0);
        wm.insert(make(1, vec![0.0, 1.0, 0.0, 0.0], "y"), 0);
        wm.insert(make(2, vec![0.0, 0.0, 1.0, 0.0], "z"), 0);

        let found = wm.query(&[0.0, 1.0, 0.0, 0.0]);
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, 1);
    }

    #[test]
    fn duplicate_id_updates_existing() {
        let mut wm = WorkingMemory::new();
        wm.insert(make(42, vec![1.0; 4], "v1"), 0);
        assert_eq!(wm.hot_len(), 1);
        wm.insert(make(42, vec![0.0; 4], "v2"), 1);
        // Toujours 1 item, mais access_count = 1 (touch)
        assert_eq!(wm.hot_len(), 1);
        assert_eq!(wm.hot[0].access_count, 1);
    }
}
