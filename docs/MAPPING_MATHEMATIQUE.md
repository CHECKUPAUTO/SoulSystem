# SoulSystem ↔ Système Mathématique d'Agent Adaptatif : Mapping & Plan d'Action

**Généré par graphify** — 69,149 nodes · 138,162 edges · 3,912 communities · 10 god nodes

---

## Résumé exécutif

| Métrique | Valeur |
|----------|--------|
| **Composants du système mathématique** | 18 |
| **Implémentés dans SoulSystem** | 16/18 (89%) |
| **Partiellement implémentés** | 2/18 (11%) |
| **Manquants** | 0/18 (0%) |

**SoulSystem bénéficie déjà quasiment intégralement du système mathématique.**  
Seuls 2 sous-composants nécessitent une unification pour atteindre 100%.

---

## Mapping détaillé

### ✅ COMPOSANTS IMPLÉMENTÉS (16/18)

#### 1. État global A_t → `souls/src/brain.rs:31` + `meta_cortex.rs:189`
- **M_t** (mémoire) : `soullink-memory-hierarchy` — 3 couches (Working→Episodic→Semantic) avec consolidation et decay exponentiel
- **θ_t** (cognitif) : `meta_cortex.rs:264` — MLP 2-couches entraîné online par SGD
- **π_t** (politique) : `soul-agent-core/src/lib.rs:320` — ReAct loop avec sélection d'action
- **G_t** (objectifs) : `soul-goaltree/src/lib.rs:29` — arbre hiérarchique d'objectifs
- **B_t** (modèle monde) : `soullink-autonomy/src/metacognition.rs:28` — SelfModel avec capacités + health
- **I_t** (initiative) : `soul-kernel/src/main.rs:56` — GoalEngine (génération réactive)
- **S_t** (social) : `soullink-senate/src/senate.rs:65` — délibération multi-expert

#### 2. Perception z_t = f_θ(o_t, M_t) → `soullink-core` (HNN)
Le Hamiltonian Neural Network (Verlet symplectic dynamics) transforme les observations en représentations internes via dynamique Hamiltonienne.

#### 3. Prédiction E_p(t) → `meta_cortex.rs:910` + `jepa/src/lib.rs:436`
- **Free Energy** : `F = prediction_error + β · complexity` (Active Inference)
- **MSE** : `||predicted - actual||²` avec historique en ring buffer
- **Détection overfit/underfit** : ajustement automatique du learning rate

#### 4. Erreur d'action δ_t → `soul-kernel/src/learning/mod.rs:28`
Q-learning avec taux d'apprentissage configurable et exploration.

#### 5. Erreur objectifs E_g → `soul-critique/src/lib.rs:104`
6 dimensions pondérées : Correctness(1.0), Safety(0.9), Robustness(0.8), Completeness(0.7), Efficiency(0.6), Clarity(0.5).

#### 6. Erreur sociale E_social → `soullink-senate/src/agreement.rs:94`
Consensus multi-agent par cosine similarity pairwise, avec seuil d'escalade.

#### 7. Incitation sociale Q_social → `ccos/src/consensus.rs:74`
Vote pondéré multi-modèle avec `agreement_ratio` et `models_in_agreement`.

#### 8. Incertitude/Curiosité U_t, C_t → `soul-cognition/src/curiosity.rs:61`
- **Novelty** : `1 - max_cosine_similarity` aux mémoires connues
- **Boredom bonus** : bonus de température quand l'erreur reste trop basse trop longtemps
- **Normalized entropy** : `H(B_t)` via Shannon entropy normalisée

#### 9. Action autonome a* → `soul-agent-core/src/lib.rs:320`
ReAct loop : observe → think → act → evaluate, avec sélection d'action parmi {agir, observer, planifier, chercher}.

#### 10. Recherche externe → `soul-webfetch/src/lib.rs:83` + `avid-scout` (753 modules)
- **WebFetcher** : HTTP avec retry, timeout, robots.txt, rate limiting
- **BrowserController** : Chrome headless via CDP (navigate, screenshot, evaluate_js, click, type)
- **AVID Scout** : 753 modules d'extraction web + swarming multi-agent

#### 11. Test de correction C_i → C* → `patch_validator.rs:52`
Pipeline complet : cargo check → cargo clippy → cargo test → restore originals → validation report.

#### 12. Récompense globale R_total → `soullink-trainer/src/lib.rs:185`
Filtrage de trajectoires multi-dimensionnel : min_quality, min_confidence, max_loss, require_success.

#### 13. Mémoire évolutive M_(t+1) → `memory-hierarchy/src/lib.rs:494`
- **Consolidation** : Working→Episodic→Semantic (Jaccard clustering)
- **Decay** : exponentiel avec demi-vie configurable (episodic: 1j, semantic: 30j)
- **Compaction** : 4-pass pipeline (Reclaim→Shrink→Collapse→Evict)

#### 14. Adaptation cognitive θ_(t+1) → `meta_cortex.rs:264` + `dream.rs:56`
- **SGD online** : backprop complète à travers MLP 2-couches
- **LR decay** : `lr *= 0.9999`, minimum 0.0001
- **Dream cycle** : marche aléatoire sur MemoryGraph, renforcement Hebbien

#### 15. Évolution politique π_(t+1) → `soullink-trainer`
Trajectoires filtrées → fine-tuning des politiques d'action.

#### 16. Auto-amélioration A_(t+1) → `soul-automodify/src/lib.rs:185`
Backup → modify → verify → rollback si échec.

#### 17. Boucle complète → `soul-agent-core` ReAct + `soul-critique` reflexion
Observer→Prédire→Agir→Comparer→Corriger→Itérer.

---

### ⚠️ PARTIEL (2/18) — Plan d'action

---

## 🔴 LACUNE 1 : Fonction d'erreur globale unifiée (E_t = Σ w_i E_i)

### Problème
6 signaux d'erreur indépendants existent mais **aucun n'est combiné** :
- `E_p` prédiction → `meta_cortex.rs` / `jepa/src/lib.rs`
- `δ_t` renforcement → `soul-kernel/src/learning/mod.rs`
- `E_g` objectifs → `soul-critique/src/lib.rs` (6 dims de qualité)
- `E_social` social → `soullink-senate/src/agreement.rs`
- `U_t` incertitude → `soul-cognition/src/curiosity.rs`
- `I_t` initiative → `soul-kernel/src/main.rs`

### Solution : Créer `soul-error-unifier`

**Fichier à créer** : `soul-error-unifier/src/lib.rs`

```rust
/// Unified global error function as defined in the mathematical system:
/// E_t = w_p * E_p + w_r * δ_t + w_g * E_g + w_s * E_social + w_u * U_t + w_i * I_t
pub struct GlobalError {
    pub total: f64,
    pub components: ErrorComponents,
    pub weights: ErrorWeights,
    pub trend: Vec<f64>, // historical for adaptive weighting
}

pub struct ErrorComponents {
    pub prediction_error: f64,    // E_p from meta_cortex/jepa
    pub reinforcement_error: f64, // δ_t from Q-learning
    pub goal_error: f64,          // E_g from soul-critique
    pub social_error: f64,        // E_social from senate
    pub uncertainty: f64,         // U_t from curiosity
    pub initiative_error: f64,    // I_t from goal generation gap
}

pub struct ErrorWeights {
    pub w_prediction: f64,    // default: 0.25
    pub w_reinforcement: f64, // default: 0.15
    pub w_goal: f64,          // default: 0.30
    pub w_social: f64,        // default: 0.15
    pub w_uncertainty: f64,   // default: 0.10
    pub w_initiative: f64,    // default: 0.05
}

impl GlobalError {
    pub fn compute(
        prediction_error: f64,
        reinforcement_error: f64,
        goal_error: f64,
        social_error: f64,
        uncertainty: f64,
        initiative_error: f64,
        weights: &ErrorWeights,
    ) -> Self {
        let total = 
            weights.w_prediction * prediction_error +
            weights.w_reinforcement * reinforcement_error +
            weights.w_goal * goal_error +
            weights.w_social * social_error +
            weights.w_uncertainty * uncertainty +
            weights.w_initiative * initiative_error;
        
        // Adaptive weight adjustment based on error trends
        // If a component is consistently high, increase its weight
        // If a component is consistently low, decrease its weight
        
        Self { total, components: ErrorComponents { ... }, weights: weights.clone(), trend: vec![] }
    }
    
    /// Adapt weights based on error history (meta-learning)
    pub fn adapt_weights(&mut self, history: &[f64]) {
        // Exponential moving average of each component
        // Redistribute weights toward higher-error components
    }
}
```

**Intégration** : Ajouter `soul-error-unifier` comme dépendance de `souls` et `soul-agent-core`, appeler `GlobalError::compute()` à chaque fin de cycle ReAct.

**Estimation** : ~300 lignes de Rust, 1 nouveau crate, ~2h de travail.

---

## 🔴 LACUNE 2 : Génération autonome de buts par curiosité (I_t → G_(t+1))

### Problème
Le `Curiosity` module (`soul-cognition/src/curiosity.rs`) existe et calcule `U_t` et `C_t`, mais **n'est pas connecté** au `GoalEngine` (`soul-kernel/src/main.rs:56`). Les buts sont générés de façon réactive (heartbeat timer, LLM response), pas par motivation intrinsèque.

### Solution : Créer le pont Curiosity → GoalEngine

**Fichier à créer** : `soul-intrinsic-motivation/src/lib.rs`

```rust
/// Bridges curiosity-driven exploration to autonomous goal generation.
/// Implements: G_(t+1) = GenerateGoal(K_t, U_t, G_t)
/// where K_t = knowledge state, U_t = uncertainty, G_t = current goals
pub struct IntrinsicMotivation {
    curiosity: Curiosity,
    goal_engine: GoalEngine,
    knowledge_state: KnowledgeState,
    exploration_budget: f64,
}

pub struct KnowledgeState {
    known_regions: HashMap<String, f64>,  // region → familiarity score
    explored_gaps: Vec<KnowledgeGap>,      // identified unknowns
    skill_coverage: f64,                   // 0..1 breadth of capabilities
}

pub struct KnowledgeGap {
    domain: String,
    current_knowledge: f64,  // 0..1
    information_value: f64,  // estimated learning gain
    reachable: bool,         // can we explore this?
}

impl IntrinsicMotivation {
    /// Core formula: I_t = f(U_t, K_t, G_t, D_t)
    /// where D_t = drive (exploration pressure)
    pub fn compute_initiative(&self) -> InitiativeSignal {
        let uncertainty = self.curiosity.current_uncertainty();
        let knowledge_gaps = self.identify_gaps();
        let drive = self.compute_drive();
        
        // High uncertainty + unexplored gaps + high drive → strong initiative
        let intensity = uncertainty * knowledge_gaps.len() as f64 * drive;
        
        InitiativeSignal { intensity, gaps: knowledge_gaps }
    }
    
    /// Generate autonomous goals from identified knowledge gaps
    /// G_(t+1) = GenerateGoal(K_t, U_t, G_t)
    pub fn generate_goal(&mut self) -> Option<Goal> {
        let gaps = self.identify_gaps();
        if gaps.is_empty() { return None; }
        
        // Select most valuable unexplored gap
        let best_gap = gaps.iter()
            .max_by(|a, b| a.information_value.partial_cmp(&b.information_value).unwrap())?;
        
        // C_t = E_p × V_information
        let curiosity_value = self.curiosity.prediction_error() * best_gap.information_value;
        
        if curiosity_value > self.exploration_budget {
            return None; // Not worth exploring
        }
        
        Some(Goal {
            description: format!("Explore domain: {}", best_gap.domain),
            priority: Priority::from(curiosity_value),
            source: GoalSource::IntrinsicMotivation,
            expected_information_gain: best_gap.information_value,
        })
    }
    
    fn compute_drive(&self) -> f64 {
        // Drive increases when:
        // - Knowledge coverage is low
        // - No active goals
        // - Prediction error is stable (boredom)
        (1.0 - self.knowledge_state.skill_coverage) * self.curiosity.boredom_bonus()
    }
}
```

**Intégration dans `souls/src/brain.rs`** :

```rust
// Dans BrainMaintenanceLoop :
if self.intrinsic_motivation.should_generate_goal() {
    if let Some(goal) = self.intrinsic_motivation.generate_goal() {
        self.goal_tree.add_goal(goal);
        tracing::info!(goal = %goal.description, "Auto-generated intrinsic goal");
    }
}
```

**Estimation** : ~400 lignes de Rust, 1 nouveau crate, ~3h de travail.

---

## 📋 Plan d'exécution

| Phase | Tâche | Fichier(s) | Estimation | Priorité |
|-------|-------|-----------|-----------|----------|
| **1** | Créer `soul-error-unifier` | `soul-error-unifier/src/lib.rs` | 300 lignes, 2h | 🔴 Haute |
| **2** | Intégrer unifier dans `souls` | `souls/src/brain.rs`, `souls/Cargo.toml` | 50 lignes, 30min | 🔴 Haute |
| **3** | Créer `soul-intrinsic-motivation` | `soul-intrinsic-motivation/src/lib.rs` | 400 lignes, 3h | 🔴 Haute |
| **4** | Intégrer motivation dans `souls` | `souls/src/brain.rs` | 50 lignes, 30min | 🔴 Haute |
| **5** | Ajouter les 2 crates au workspace | `Cargo.toml` | 5 lignes, 5min | 🔴 Haute |
| **6** | Tests unitaires | `soul-error-unifier/tests/`, `soul-intrinsic-motivation/tests/` | 200 lignes, 1h | 🟡 Moyenne |
| **7** | Tests d'intégration | `tests/` | 100 lignes, 30min | 🟡 Moyenne |
| **8** | Documentation | `docs/` | 30min | 🟢 Basse |

**Total estimé** : ~1100 lignes de Rust, ~8h de travail.

---

## 🎯 Résultat final attendu

Après implémentation, la formule générale sera pleinement réalisée :

```
Agent_(t+1) = Agent_t + Apprentissage(
    E_p·w_p + δ_t·w_r + E_g·w_g + E_social·w_s + U_t·w_u + I_t·w_i
)
```

Avec :
- **Tous les signaux d'erreur unifiés** dans une seule fonction de perte
- **Génération de buts autonome** pilotée par la curiosité et l'incertitude
- **Boucle complète** 100% conforme au système mathématique
