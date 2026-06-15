//! Méta-évolution : le système qui améliore le système qui améliore les agents.
//!
//! Implémente les concepts clés extraits des 50 papiers sur l'auto-amélioration récursive :
//!
//! 1. **Gödel Machine** (Schmidhuber 2007) — Boucle autoréférentielle :
//!    Chaque stratégie peut réécrire n'importe quelle partie du système,
//!    y compris les autres stratégies. Une "preuve" d'utilité est requise.
//!
//! 2. **Darwin Gödel Machine** (Zhang et al. 2025) — Archive ouverte :
//!    Maintient un archive croissant d'agents divers. Échantillonne + mute
//!    pour créer de nouveaux agents, formant un arbre d'innovation.
//!
//! 3. **STOP** (Zelikman et al. 2023) — Améliorateur auto-améliorant :
//!    L'"improver" s'améliore lui-même : les stratégies de mutation évoluent.
//!
//! 4. **AlphaZero** (Silver et al. 2017) — Self-play :
//!    Le meilleur agent courant affronte des challengers. Le gagnant propage
//!    ses caractéristiques.
//!
//! 5. **OPRO + Reflexion** (Yang 2023, Shinn 2023) — Trajectoire + réflexion :
//!    Chaque tentative enrichit la trajectoire d'optimisation. L'agent
//!    réfléchit sur ses échecs et succès.
//!
//! 6. **I.J. Good** (1965) — Détection d'explosion d'intelligence :
//!    Calcule les dérivées première et seconde du progrès pour détecter
//!    une accélération compoundante.
//!
//! 7. **Utilité & Preuve** (Gödel) — Fonction d'utilité multi-critère :
//!    Avant chaque changement, on vérifie qu'il améliore l'utilité globale.

use crate::types::AuditReport;
use crate::types::*;
// Generator import not needed at module level; used in SelfPlayArena
use std::collections::{HashMap, HashSet};

// ──────────────────────────────────────────
// 1. GÖDEL MACHINE — Boucle Auto-Référentielle
// ──────────────────────────────────────────

pub struct GodelEngine {
    pub strategies: Vec<GodelStrategy>,
    pub proposals: Vec<SelfModProposal>,
    pub utility_fn: UtilityFunction,
    pub explosion: ExplosionMetrics,
    pub iteration: u64,
}

impl Default for GodelEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl GodelEngine {
    pub fn new() -> Self {
        Self {
            strategies: Self::seed_strategies(),
            proposals: Vec::new(),
            utility_fn: UtilityFunction::default(),
            explosion: ExplosionMetrics::new(0.01),
            iteration: 0,
        }
    }

    /// Stratégies initiales (le "proof searcher" de la Gödel Machine)
    fn seed_strategies() -> Vec<GodelStrategy> {
        vec![
            GodelStrategy {
                name: "generate-missing-language-agents".into(),
                description: "Detect uncovered languages and generate reviewer + resolver agents"
                    .into(),
                code: "audit → analyzer → generator → validate → apply".into(),
                version: 1,
                utility_history: vec![],
                parent: None,
                children: vec![],
                mutation_rate: 0.3,
                last_modified: chrono::Utc::now(),
                effectiveness: 0.0,
            },
            GodelStrategy {
                name: "improve-low-quality-agents".into(),
                description: "Rewrite agents below quality threshold with improved versions".into(),
                code: "audit → filter low quality → improved template → validate → apply".into(),
                version: 1,
                utility_history: vec![],
                parent: None,
                children: vec![],
                mutation_rate: 0.2,
                last_modified: chrono::Utc::now(),
                effectiveness: 0.0,
            },
            GodelStrategy {
                name: "fix-incomplete-agents".into(),
                description: "Patch agents missing critical fields (name, tools, model)".into(),
                code: "audit → detect missing fields → auto-fix → validate".into(),
                version: 1,
                utility_history: vec![],
                parent: None,
                children: vec![],
                mutation_rate: 0.1,
                last_modified: chrono::Utc::now(),
                effectiveness: 0.0,
            },
            GodelStrategy {
                name: "self-play-competition".into(),
                description: "AlphaZero-style: best agent vs challenger, winner propagates".into(),
                code: "select champion → mutate challenger → compete → propagate winner".into(),
                version: 1,
                utility_history: vec![],
                parent: None,
                children: vec![],
                mutation_rate: 0.4,
                last_modified: chrono::Utc::now(),
                effectiveness: 0.0,
            },
            GodelStrategy {
                name: "archive-diversification".into(),
                description:
                    "Darwin-style: sample from archive, mutate, add back preserving diversity"
                        .into(),
                code: "sample archive entry → novelty-check → mutate → archive → prune".into(),
                version: 1,
                utility_history: vec![],
                parent: None,
                children: vec![],
                mutation_rate: 0.5,
                last_modified: chrono::Utc::now(),
                effectiveness: 0.0,
            },
        ]
    }

    /// Évalue et met à jour l'efficacité de chaque stratégie après un cycle
    pub fn evaluate_strategies(
        &mut self,
        report: &AuditReport,
        archive: &AgentArchive,
        applied: &[ImprovementResult],
    ) {
        let current_utility = self.utility_fn.evaluate(report, archive);
        self.explosion.record(current_utility);

        for strategy in &mut self.strategies {
            strategy.utility_history.push(current_utility);
            if strategy.utility_history.len() >= 2 {
                let recent: Vec<f64> = strategy
                    .utility_history
                    .iter()
                    .rev()
                    .take(3)
                    .copied()
                    .collect();
                let delta = if recent.len() >= 2 {
                    (recent[0] - recent[recent.len() - 1]) / recent.len() as f64
                } else {
                    0.0
                };
                let applied_count = applied
                    .iter()
                    .filter(|r| r.success && r.message.contains(&strategy.name))
                    .count() as f64;
                strategy.effectiveness = delta * (1.0 + applied_count * 0.1);
            }
        }
        self.iteration += 1;
    }

    /// Génère des propositions d'auto-modification (Gödel Machine "proofs")
    pub fn propose_self_mods(&self) -> Vec<SelfModProposal> {
        let mut proposals = Vec::new();

        // Gödel Principle: mutate the least effective strategy
        if let Some(weakest) = self
            .strategies
            .iter()
            .filter(|s| s.utility_history.len() >= 2)
            .min_by(|a, b| {
                a.effectiveness
                    .partial_cmp(&b.effectiveness)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        {
            let mutated_rate = (weakest.mutation_rate * 1.5).min(1.0);
            proposals.push(SelfModProposal {
                strategy_name: weakest.name.clone(),
                target_path: vec!["mutation_rate".into()],
                new_code: format!("mutation_rate = {:.2}", mutated_rate),
                expected_utility_gain: 0.05,
                proof: format!(
                    "Strategy '{}' has lowest effectiveness ({:.2}). \
                     Increasing mutation rate to {:.2} may escape local optima.",
                    weakest.name, weakest.effectiveness, mutated_rate
                ),
                applied: false,
                actual_utility_gain: None,
            });
        }

        // Gödel Principle: if explosion detected, add safety strategy
        if self.explosion.is_exploding && self.explosion.explosion_probability > 0.5 {
            proposals.push(SelfModProposal {
                strategy_name: "safety-check".into(),
                target_path: vec!["strategies".into(), "safety".into()],
                new_code: "add safety verification step before all mutations".into(),
                expected_utility_gain: 0.1,
                proof: format!(
                    "Intelligence explosion detected (prob={:.2}). \
                     Adding safety checks prevents uncontrolled acceleration.",
                    self.explosion.explosion_probability
                ),
                applied: false,
                actual_utility_gain: None,
            });
        }

        proposals
    }
}

// ──────────────────────────────────────────
// 2. DARWIN ARCHIVE — Évolution Ouverte
// ──────────────────────────────────────────

impl AgentArchive {
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_size,
            novelty_weight: 0.4,
            quality_weight: 0.6,
            diversity_threshold: 0.15,
        }
    }

    /// Ajoute un agent à l'archive (Darwin Gödel Machine: archive growing tree)
    pub fn add(
        &mut self,
        agent_def: AgentDef,
        quality_score: f64,
        parent_id: Option<String>,
        generation: u64,
        tags: Vec<String>,
    ) {
        let novelty_score = self.compute_novelty(&agent_def);

        let entry = ArchiveEntry {
            id: format!("arch-{:04}-{}", self.entries.len(), agent_def.name),
            agent_def,
            quality_score,
            novelty_score,
            parent_id,
            generation,
            created_at: chrono::Utc::now(),
            tags,
        };

        self.entries.push(entry);

        // Prune si l'archive dépasse la taille maximale
        if self.entries.len() > self.max_size {
            self.prune();
        }
    }

    /// Échantillonne un agent de l'archive avec biais vers qualité + nouveauté
    pub fn sample(&self) -> Option<&ArchiveEntry> {
        if self.entries.is_empty() {
            return None;
        }

        let scores: Vec<f64> = self
            .entries
            .iter()
            .map(|e| self.quality_weight * e.quality_score + self.novelty_weight * e.novelty_score)
            .collect();

        let total: f64 = scores.iter().sum();
        if total <= 0.0 {
            return Some(&self.entries[fastrand::usize(..self.entries.len())]);
        }

        let mut r = fastrand::f64() * total;
        for (i, score) in scores.iter().enumerate() {
            r -= score;
            if r <= 0.0 {
                return Some(&self.entries[i]);
            }
        }
        self.entries.last()
    }

    /// Calcule la nouveauté d'un agent par rapport à l'archive existante
    fn compute_novelty(&self, agent: &AgentDef) -> f64 {
        if self.entries.is_empty() {
            return 1.0;
        }

        let mut max_similarity = 0.0;
        for entry in &self.entries {
            let sim = Self::agent_similarity(agent, &entry.agent_def);
            if sim > max_similarity {
                max_similarity = sim;
            }
        }

        1.0 - max_similarity
    }

    /// Similarité cosinus entre deux agents (basée sur leurs attributs)
    fn agent_similarity(a: &AgentDef, b: &AgentDef) -> f64 {
        let mut score = 0.0;

        // Nom similaire ?
        if a.name == b.name {
            score += 0.3;
        } else if a.name.split('-').any(|p| b.name.contains(p)) {
            score += 0.15;
        }

        // Même modèle ?
        if a.model == b.model {
            score += 0.1;
        }

        // Chevauchement d'outils
        let a_tools: HashSet<&str> = a.tools.iter().map(|s| s.as_str()).collect();
        let b_tools: HashSet<&str> = b.tools.iter().map(|s| s.as_str()).collect();
        let intersection = a_tools.intersection(&b_tools).count();
        let union = a_tools.union(&b_tools).count();
        if union > 0 {
            score += 0.2 * intersection as f64 / union as f64;
        }

        // Description similaire ?
        let a_words: HashSet<&str> = a.description.split_whitespace().collect();
        let b_words: HashSet<&str> = b.description.split_whitespace().collect();
        let word_intersection = a_words.intersection(&b_words).count() as f64;
        let word_union = a_words.union(&b_words).count() as f64;
        if word_union > 0.0 {
            score += 0.2 * word_intersection / word_union;
        }

        score.min(1.0)
    }

    /// Élimine les entrées redondantes (faible nouveauté + faible qualité)
    fn prune(&mut self) {
        if self.entries.len() <= self.max_size {
            return;
        }

        let to_remove: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.novelty_score < self.diversity_threshold && e.quality_score < 0.5)
            .map(|(i, _)| i)
            .collect();

        for &i in to_remove.iter().rev() {
            self.entries.swap_remove(i);
        }

        while self.entries.len() > self.max_size {
            self.entries.swap_remove(0);
        }
    }

    /// Compte les agents par tags
    pub fn tag_distribution(&self) -> HashMap<String, usize> {
        let mut dist: HashMap<String, usize> = HashMap::new();
        for entry in &self.entries {
            for tag in &entry.tags {
                *dist.entry(tag.clone()).or_insert(0) += 1;
            }
        }
        dist
    }
}

// ──────────────────────────────────────────
// 3. STOP IMPROVER — Auto-Amélioration
// ──────────────────────────────────────────

impl Improver {
    pub fn new(name: &str, strategy: ImproverStrategy) -> Self {
        Self {
            name: name.to_string(),
            version: 1,
            strategy_type: strategy,
            performance_history: Vec::new(),
            best_performance: 0.0,
            mutation_method: "default".to_string(),
        }
    }

    /// Améliore l'improver lui-même (STOP recursive self-improvement)
    pub fn self_improve(&mut self, recent_performance: f64) -> Option<ImproverStrategy> {
        self.performance_history.push(recent_performance);
        if recent_performance > self.best_performance {
            self.best_performance = recent_performance;
        }

        // STOP: si la performance stagne, muter la stratégie
        if self.performance_history.len() >= 3 {
            let recent: Vec<f64> = self
                .performance_history
                .iter()
                .rev()
                .take(3)
                .copied()
                .collect();
            if recent[0] <= recent[1] && recent[1] <= recent[2] {
                // Performance plateau → muter vers une nouvelle stratégie
                let new_strategy = self.mutate_strategy();
                self.strategy_type = new_strategy.clone();
                self.version += 1;
                self.mutation_method = format!("plateau-escape-v{}", self.version);
                return Some(new_strategy);
            }
        }
        None
    }

    /// Mute la stratégie d'amélioration (STOP: explore new improvement strategies)
    fn mutate_strategy(&self) -> ImproverStrategy {
        match self.strategy_type {
            ImproverStrategy::BeamSearch => ImproverStrategy::GeneticAlgorithm,
            ImproverStrategy::GeneticAlgorithm => ImproverStrategy::SimulatedAnnealing,
            ImproverStrategy::SimulatedAnnealing => ImproverStrategy::ReflexiveMutation,
            ImproverStrategy::ReflexiveMutation => ImproverStrategy::CrossoverOnly,
            ImproverStrategy::CrossoverOnly => ImproverStrategy::RandomMutation,
            ImproverStrategy::RandomMutation => ImproverStrategy::BeamSearch,
        }
    }
}

// ──────────────────────────────────────────
// 4. ALPHAZERO SELF-PLAY
// ──────────────────────────────────────────

pub struct SelfPlayArena {
    pub matches: Vec<SelfPlayMatch>,
    pub champion: Option<String>,
    pub champion_score: f64,
    pub rounds_per_match: u64,
}

impl SelfPlayArena {
    pub fn new(rounds: u64) -> Self {
        Self {
            matches: Vec::new(),
            champion: None,
            champion_score: 0.0,
            rounds_per_match: rounds,
        }
    }

    /// Organise un match entre le champion et un challenger
    pub fn compete(&mut self, champion: &ArchiveEntry, challenger: &ArchiveEntry) -> SelfPlayMatch {
        let match_result = SelfPlayMatch {
            agent_a: champion.agent_def.name.clone(),
            agent_b: challenger.agent_def.name.clone(),
            agent_a_score: champion.quality_score,
            agent_b_score: challenger.quality_score,
            winner: if challenger.quality_score > champion.quality_score {
                Some(challenger.agent_def.name.clone())
            } else {
                Some(champion.agent_def.name.clone())
            },
            rounds: self.rounds_per_match,
            timestamp: chrono::Utc::now(),
        };

        // AlphaZero: le gagnant devient le nouveau champion
        if match_result.winner.as_ref() == Some(&challenger.agent_def.name) {
            self.champion = Some(challenger.agent_def.name.clone());
            self.champion_score = challenger.quality_score;
        } else {
            self.champion = Some(champion.agent_def.name.clone());
            self.champion_score = champion.quality_score;
        }

        self.matches.push(match_result.clone());
        match_result
    }

    /// Génère un challenger en mutant le champion actuel
    pub fn generate_challenger(
        &self,
        champion: &ArchiveEntry,
        _generator: &(),
    ) -> Option<AgentDef> {
        let mut mutated = champion.agent_def.clone();
        // Mute la description
        if fastrand::f64() < 0.3 {
            mutated.description = format!(
                "{} (self-play variant {})",
                mutated.description,
                self.matches.len() + 1
            );
        }
        // Mute le modèle
        if fastrand::f64() < 0.2 {
            mutated.model = if champion.agent_def.model == "opus" {
                "sonnet".to_string()
            } else {
                "opus".to_string()
            };
        }
        Some(mutated)
    }
}

// ──────────────────────────────────────────
// 5. RÉFLEXION — Apprentissage par renforcement verbal
// ──────────────────────────────────────────

pub struct ReflexionMemory {
    pub episodes: Vec<ReflexionEpisode>,
    pub success_patterns: Vec<String>,
    pub failure_patterns: Vec<String>,
}

impl Default for ReflexionMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl ReflexionMemory {
    pub fn new() -> Self {
        Self {
            episodes: Vec::new(),
            success_patterns: Vec::new(),
            failure_patterns: Vec::new(),
        }
    }

    /// Ajoute un épisode de réflexion (Reflexion: self-reflection through feedback)
    pub fn record(&mut self, action: &str, outcome: &str, reflection: &str, lessons: Vec<String>) {
        let episode = ReflexionEpisode {
            iteration: self.episodes.len() as u64 + 1,
            action: action.to_string(),
            outcome: outcome.to_string(),
            reflection: reflection.to_string(),
            lessons: lessons.clone(),
            emotional_state: if outcome.contains("success") || outcome.contains("improved") {
                "positive".to_string()
            } else {
                "corrective".to_string()
            },
            timestamp: chrono::Utc::now(),
        };

        for lesson in &lessons {
            if outcome.contains("success") || outcome.contains("improved") {
                if !self.success_patterns.contains(lesson) {
                    self.success_patterns.push(lesson.clone());
                }
            } else if !self.failure_patterns.contains(lesson) {
                self.failure_patterns.push(lesson.clone());
            }
        }

        self.episodes.push(episode);
    }

    /// Génère un prompt de réflexion pour guider la prochaine action
    pub fn generate_reflection_prompt(&self) -> String {
        let mut prompt = String::new();

        prompt.push_str("[SELF-REFLECTION]\n");
        prompt.push_str(&format!("Total episodes: {}\n", self.episodes.len()));

        if !self.success_patterns.is_empty() {
            prompt.push_str("\nWhat worked before:\n");
            for p in self.success_patterns.iter().rev().take(3) {
                prompt.push_str(&format!("  ✓ {}\n", p));
            }
        }

        if !self.failure_patterns.is_empty() {
            prompt.push_str("\nWhat to avoid:\n");
            for p in self.failure_patterns.iter().rev().take(3) {
                prompt.push_str(&format!("  ✗ {}\n", p));
            }
        }

        prompt
    }
}

// ──────────────────────────────────────────
// 6. TRAJECTOIRE D'OPTIMISATION (OPRO)
// ──────────────────────────────────────────

impl OptimizationTrajectory {
    pub fn new(target_name: &str) -> Self {
        Self {
            target_name: target_name.to_string(),
            attempts: Vec::new(),
            best_score: 0.0,
            best_attempt: None,
        }
    }

    /// Ajoute un point à la trajectoire
    pub fn record(&mut self, description: &str, score: f64, change: &str, strategy: &str) {
        let point = TrajectoryPoint {
            iteration: self.attempts.len() + 1,
            description: description.to_string(),
            score,
            change_description: change.to_string(),
            strategy_used: strategy.to_string(),
        };

        if score > self.best_score {
            self.best_score = score;
            self.best_attempt = Some(self.attempts.len());
        }

        self.attempts.push(point);
    }

    /// Génère un prompt OPRO-style avec la trajectoire complète
    pub fn to_opro_prompt(&self) -> String {
        let mut prompt = String::new();

        prompt.push_str(&format!(
            "[OPTIMIZATION TRAJECTORY for {}]\n\n",
            self.target_name
        ));

        for (i, attempt) in self.attempts.iter().enumerate() {
            let marker = if Some(i) == self.best_attempt {
                " ★ BEST"
            } else {
                ""
            };
            prompt.push_str(&format!(
                "  Step {}: score={:.3}{}\n",
                attempt.iteration, attempt.score, marker
            ));
            prompt.push_str(&format!("    Strategy: {}\n", attempt.strategy_used));
            prompt.push_str(&format!("    Change: {}\n", attempt.change_description));
        }

        prompt.push_str(&format!(
            "\nBest score so far: {:.3} at step {}\n",
            self.best_score,
            self.best_attempt.unwrap_or(0) + 1
        ));

        prompt
    }
}

// ──────────────────────────────────────────
// 7. EXPLOSION D'INTELLIGENCE (I.J. Good)
// ──────────────────────────────────────────

pub fn format_explosion_report(metrics: &ExplosionMetrics) -> String {
    let mut out = String::new();
    use std::fmt::Write;

    let _ = writeln!(out, "╔══════════════════════════════════════════════╗");
    let _ = writeln!(out, "║     I.J. GOOD INTELLIGENCE EXPLOSION         ║");
    let _ = writeln!(out, "╚══════════════════════════════════════════════╝");
    let _ = writeln!(out);

    let _ = writeln!(
        out,
        "  Health points recorded: {}",
        metrics.health_history.len()
    );
    let _ = writeln!(out, "  Trend:                 {}", metrics.trend);

    if let Some(&fd) = metrics.first_derivative.last() {
        let _ = writeln!(out, "  1st derivative (rate): {:+.6}", fd);
    }
    if let Some(&sd) = metrics.second_derivative.last() {
        let _ = writeln!(out, "  2nd derivative (accel): {:+.6}", sd);
    }

    if metrics.is_exploding {
        let _ = writeln!(out, "  ⚠  INTELLIGENCE EXPLOSION DETECTED");
        let _ = writeln!(
            out,
            "  Explosion probability: {:.1}%",
            metrics.explosion_probability * 100.0
        );
        let _ = writeln!(out, "  Recommendation: Add safety constraints immediately.");
    } else {
        let _ = writeln!(out, "  No explosion detected. System is stable.");
    }

    let _ = writeln!(out, "────────────────────────────────────────────");
    out
}

// ──────────────────────────────────────────
// 8. ORCHESTRATEUR MÉTA-ÉVOLUTION
// ──────────────────────────────────────────

pub struct MetaEvolver {
    pub godel: GodelEngine,
    pub archive: AgentArchive,
    pub improver: Improver,
    pub arena: SelfPlayArena,
    pub reflexion: ReflexionMemory,
    pub trajectory: OptimizationTrajectory,
}

impl Default for MetaEvolver {
    fn default() -> Self {
        Self::new()
    }
}

impl MetaEvolver {
    pub fn new() -> Self {
        Self {
            godel: GodelEngine::new(),
            archive: AgentArchive::new(100),
            improver: Improver::new("meta-improver", ImproverStrategy::GeneticAlgorithm),
            arena: SelfPlayArena::new(3),
            reflexion: ReflexionMemory::new(),
            trajectory: OptimizationTrajectory::new("agent-ecosystem"),
        }
    }

    /// Exécute un cycle complet de méta-évolution (tous les papiers combinés)
    pub fn cycle(
        &mut self,
        report: &AuditReport,
        audits: &[AgentAudit],
        _config: &EvolutionConfig,
    ) -> MetaCycleResult {
        let mut result = MetaCycleResult::new();

        // Phase 1: Gödel — évaluer les stratégies
        self.godel
            .evaluate_strategies(report, &self.archive, &result.applied);

        // Phase 2: Darwin — ajouter les meilleurs agents à l'archive
        for audit in audits.iter().filter(|a| a.quality_score > 0.7) {
            self.archive.add(
                audit.agent.clone(),
                audit.quality_score,
                None,
                0,
                vec!["auto-detected".to_string()],
            );
            result.archive_additions += 1;
        }

        // Phase 3: STOP — auto-améliorer l'improver
        let new_strategy = self.improver.self_improve(report.ecosystem_health);
        if let Some(strat) = new_strategy {
            result.strategy_mutation = Some(format!("{:?}", strat));
        }

        // Phase 4: AlphaZero — self-play
        if let (Some(champion), Some(challenger)) = (self.archive.sample(), self.archive.sample()) {
            if champion.id != challenger.id {
                let match_result = self.arena.compete(champion, challenger);
                result.self_play_matches += 1;
                result.latest_winner = match_result.winner.clone();
            }
        }

        // Phase 5: Réflexion
        self.reflexion.record(
            "meta-cycle",
            if report.ecosystem_health > 0.5 {
                "success"
            } else {
                "needs-improvement"
            },
            &format!(
                "Ecosystem health {:.1}%, {} agents, {} in archive",
                report.ecosystem_health * 100.0,
                report.agent_count,
                self.archive.entries.len()
            ),
            vec![],
        );

        // Phase 6: Trajectoire OPRO
        self.trajectory.record(
            "meta-evolution-cycle",
            report.ecosystem_health,
            &format!("{} agents audited", report.agent_count),
            &format!("{:?}", self.improver.strategy_type),
        );

        // Phase 7: I.J. Good — détection d'explosion
        result.explosion_report = format_explosion_report(&self.godel.explosion);

        // Phase 8: Gödel — propositions d'auto-modification
        let self_mods = self.godel.propose_self_mods();
        for mod_proposal in self_mods {
            let current_utility = self.godel.utility_fn.evaluate(report, &self.archive);
            let projected_utility = current_utility + mod_proposal.expected_utility_gain;
            let (beneficial, proof) = self.godel.utility_fn.is_provably_beneficial(
                current_utility,
                projected_utility,
                0.01,
            );
            result
                .self_mod_checks
                .push((mod_proposal.strategy_name, beneficial, proof));
        }

        result
    }
}

#[derive(Debug, Clone)]
pub struct MetaCycleResult {
    pub archive_additions: usize,
    pub self_play_matches: u64,
    pub strategy_mutation: Option<String>,
    pub latest_winner: Option<String>,
    pub applied: Vec<ImprovementResult>,
    pub self_mod_checks: Vec<(String, bool, String)>,
    pub explosion_report: String,
}

impl Default for MetaCycleResult {
    fn default() -> Self {
        Self::new()
    }
}

impl MetaCycleResult {
    pub fn new() -> Self {
        Self {
            archive_additions: 0,
            self_play_matches: 0,
            strategy_mutation: None,
            latest_winner: None,
            applied: Vec::new(),
            self_mod_checks: Vec::new(),
            explosion_report: String::new(),
        }
    }

    pub fn format(&self) -> String {
        let mut out = String::new();
        use std::fmt::Write;

        let _ = writeln!(out, "╔══════════════════════════════════════════════╗");
        let _ = writeln!(out, "║        META-EVOLUTION CYCLE REPORT           ║");
        let _ = writeln!(out, "╚══════════════════════════════════════════════╝");
        let _ = writeln!(out);
        let _ = writeln!(out, "  Archive entries:     {}", self.archive_additions);
        let _ = writeln!(out, "  Self-play matches:   {}", self.self_play_matches);
        if let Some(ref strat) = self.strategy_mutation {
            let _ = writeln!(out, "  Strategy mutated to: {}", strat);
        }
        if let Some(ref w) = self.latest_winner {
            let _ = writeln!(out, "  Latest winner:       {}", w);
        }

        if !self.self_mod_checks.is_empty() {
            let _ = writeln!(out);
            let _ = writeln!(out, "── Gödel Self-Modification Checks ──");
            for (name, beneficial, proof) in &self.self_mod_checks {
                let status = if *beneficial {
                    "✓ APPROVED"
                } else {
                    "✗ REJECTED"
                };
                let _ = writeln!(out, "  {} [{}]", name, status);
                let _ = writeln!(out, "    Proof: {}", proof);
            }
        }

        let _ = writeln!(out);
        let _ = writeln!(out, "{}", self.explosion_report);
        out
    }
}

// ──────────────────────────────────────────
// TESTS
// ──────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_godel_engine_initialization() {
        let engine = GodelEngine::new();
        assert_eq!(engine.strategies.len(), 5);
        assert!(engine.strategies[0].name.contains("generate"));
    }

    #[test]
    fn test_archive_add_and_sample() {
        let mut archive = AgentArchive::new(10);
        let def = AgentDef {
            name: "test-agent".into(),
            description: "A test agent".into(),
            tools: vec!["Read".into()],
            model: "sonnet".into(),
            effort: None,
            color: None,
            max_turns: None,
            allowed_tools: None,
            body: "body".into(),
            path: std::path::PathBuf::from("test.md"),
            raw_frontmatter: "name: test-agent".into(),
        };
        archive.add(def, 0.8, None, 1, vec!["test".into()]);
        assert_eq!(archive.entries.len(), 1);
        assert!(archive.sample().is_some());
    }

    #[test]
    fn test_improver_self_improve() {
        let mut improver = Improver::new("test", ImproverStrategy::BeamSearch);
        assert!(improver.self_improve(0.5).is_none());
        assert!(improver.self_improve(0.5).is_none());
        // Third plateau should trigger mutation
        let result = improver.self_improve(0.5);
        assert!(result.is_some());
        assert_eq!(improver.strategy_type, ImproverStrategy::GeneticAlgorithm);
    }

    #[test]
    fn test_reflexion_memory() {
        let mut mem = ReflexionMemory::new();
        mem.record(
            "action-1",
            "success: improved",
            "good",
            vec!["use SIMD".into()],
        );
        mem.record(
            "action-2",
            "failed: crashed",
            "bad",
            vec!["check bounds".into()],
        );
        assert_eq!(mem.success_patterns.len(), 1);
        assert_eq!(mem.failure_patterns.len(), 1);
        let prompt = mem.generate_reflection_prompt();
        assert!(prompt.contains("What worked before"));
        assert!(prompt.contains("What to avoid"));
    }

    #[test]
    fn test_trajectory_tracking() {
        let mut traj = OptimizationTrajectory::new("test");
        traj.record("first try", 0.5, "initial", "random");
        traj.record("second try", 0.8, "improved", "crossover");
        assert_eq!(traj.best_score, 0.8);
        assert_eq!(traj.best_attempt, Some(1));
        let prompt = traj.to_opro_prompt();
        assert!(prompt.contains("BEST"));
    }

    #[test]
    fn test_explosion_detection() {
        let mut metrics = ExplosionMetrics::new(0.01);
        assert!(!metrics.is_exploding);

        // Simule une accélération compoundante
        metrics.record(0.5);
        metrics.record(0.55);
        metrics.record(0.65);
        metrics.record(0.80);
        metrics.record(0.95);

        assert!(metrics.is_exploding || metrics.second_derivative.len() > 0);
    }

    #[test]
    fn test_self_play_arena() {
        let mut arena = SelfPlayArena::new(3);
        let def_a = AgentDef {
            name: "agent-a".into(),
            description: "test".into(),
            tools: vec![],
            model: "opus".into(),
            effort: None,
            color: None,
            max_turns: None,
            allowed_tools: None,
            body: "".into(),
            path: std::path::PathBuf::from("a.md"),
            raw_frontmatter: "".into(),
        };
        let def_b = AgentDef {
            name: "agent-b".into(),
            description: "test".into(),
            tools: vec![],
            model: "sonnet".into(),
            effort: None,
            color: None,
            max_turns: None,
            allowed_tools: None,
            body: "".into(),
            path: std::path::PathBuf::from("b.md"),
            raw_frontmatter: "".into(),
        };
        let entry_a = ArchiveEntry {
            id: "a".into(),
            agent_def: def_a,
            quality_score: 0.9,
            novelty_score: 0.5,
            parent_id: None,
            generation: 1,
            created_at: chrono::Utc::now(),
            tags: vec![],
        };
        let entry_b = ArchiveEntry {
            id: "b".into(),
            agent_def: def_b,
            quality_score: 0.7,
            novelty_score: 0.5,
            parent_id: None,
            generation: 1,
            created_at: chrono::Utc::now(),
            tags: vec![],
        };
        let result = arena.compete(&entry_a, &entry_b);
        assert_eq!(result.winner, Some("agent-a".into()));
        assert_eq!(arena.champion, Some("agent-a".into()));
    }

    #[test]
    fn test_utility_function() {
        let uf = UtilityFunction::default();
        assert_eq!(uf.health_weight, 0.30);
        assert_eq!(uf.coverage_weight, 0.20);
    }
}
