use crate::sandbox::SandboxResult;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

// ─────────────────────────────────────────────
// Rôles dynamiques
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Role {
    Explorer,
    Optimizer,
    Stabilizer,
    MemorySpecialist,
    Reasoner,
}

impl Role {
    pub fn all() -> Vec<Role> {
        vec![
            Role::Explorer,
            Role::Optimizer,
            Role::Stabilizer,
            Role::MemorySpecialist,
            Role::Reasoner,
        ]
    }

    pub fn random() -> Self {
        let roles = Self::all();
        let idx = rand::thread_rng().gen_range(0..roles.len());
        roles[idx].clone()
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Role::Explorer => write!(f, "Explorer"),
            Role::Optimizer => write!(f, "Optimizer"),
            Role::Stabilizer => write!(f, "Stabilizer"),
            Role::MemorySpecialist => write!(f, "MemorySpecialist"),
            Role::Reasoner => write!(f, "Reasoner"),
        }
    }
}

// ─────────────────────────────────────────────
// Métriques MESURÉES (pas des floats aléatoires)
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeasuredMetrics {
    /// Code a compilé avec succès
    pub compiles: bool,
    /// Code a tourné sans crash
    pub runs_ok: bool,
    /// Tests passés / total
    pub tests_passed: usize,
    pub tests_total: usize,
    /// Temps de compilation (ms)
    pub compile_time_ms: u64,
    /// Temps d'exécution (ms)
    pub run_time_ms: u64,
    /// Score de performance calculé [0.0 - 1.0]
    pub performance: f32,
    /// Score de stabilité calculé [0.0 - 1.0]
    pub stability: f32,
}

impl Default for MeasuredMetrics {
    fn default() -> Self {
        Self {
            compiles: false,
            runs_ok: false,
            tests_passed: 0,
            tests_total: 0,
            compile_time_ms: 0,
            run_time_ms: 0,
            performance: 0.0,
            stability: 0.0,
        }
    }
}

impl MeasuredMetrics {
    /// Mettre à jour depuis un résultat sandbox
    pub fn update_from_sandbox(&mut self, result: &SandboxResult) {
        self.compiles = result.compiled;
        self.runs_ok = result.ran_ok;
        self.tests_passed = result.tests_passed;
        self.tests_total = result.tests_total;
        self.compile_time_ms = result.compile_time_ms;
        self.run_time_ms = result.run_time_ms;
        self.performance = result.performance_score();
        self.stability = result.stability_score();
    }
}

// ─────────────────────────────────────────────
// Agent
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub role: Role,
    pub fitness: f32,
    /// Créativité = diversité des mutations tentées
    pub creativity: f32,
    /// Efficacité mémoire = taille du code / score obtenu
    pub memory_efficiency: f32,
    /// Métriques RÉELLES mesurées par le sandbox
    pub metrics: MeasuredMetrics,
    pub generation: u64,
    /// Code Rust actuel de l'agent
    pub code: String,
    /// Historique des mutations
    pub mutations_history: Vec<String>,
    pub alive: bool,
    /// Nombre de cycles consécutifs sans amélioration
    pub stagnation_count: usize,
    /// Meilleure fitness atteinte
    pub best_fitness: f32,
}

impl Agent {
    pub fn new(role: Role) -> Self {
        let mut rng = rand::thread_rng();
        Self {
            id: Uuid::new_v4().to_string(),
            role,
            fitness: 0.0,
            creativity: rng.gen_range(0.1..0.5),
            memory_efficiency: 0.0,
            metrics: MeasuredMetrics::default(),
            generation: 0,
            code: String::new(),
            mutations_history: Vec::new(),
            alive: true,
            stagnation_count: 0,
            best_fitness: 0.0,
        }
    }

    /// Calcule la fitness à partir des métriques MESURÉES
    /// fitness = performance * 0.4 + stability * 0.3 + creativity * 0.2 + memory_eff * 0.1
    pub fn calculate_fitness(&mut self) -> f32 {
        let performance = self.metrics.performance;
        let stability = self.metrics.stability;

        // Créativité = nb de types de mutations différentes tentées (normalisé)
        let unique_types = self
            .mutations_history
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len();
        self.creativity = (unique_types as f32 / 5.0).min(1.0);

        // Efficacité mémoire = performance / taille du code (plus petit = mieux)
        let code_size = self.code.len().max(1) as f32;
        self.memory_efficiency = if performance > 0.0 {
            (performance / (code_size / 500.0).max(1.0)).min(1.0)
        } else {
            0.0
        };

        let _old_fitness = self.fitness;
        self.fitness = performance * 0.4
            + stability * 0.3
            + self.creativity * 0.2
            + self.memory_efficiency * 0.1;

        // Tracking de stagnation
        if self.fitness > self.best_fitness + 0.005 {
            self.best_fitness = self.fitness;
            self.stagnation_count = 0;
        } else {
            self.stagnation_count += 1;
        }

        self.fitness
    }

    /// Mettre à jour les métriques depuis le sandbox
    pub fn update_from_sandbox(&mut self, result: &SandboxResult) {
        self.metrics.update_from_sandbox(result);
        self.calculate_fitness();
    }

    /// Appliquer une mutation (met à jour le code, recalcule après sandbox)
    pub fn apply_mutation(&mut self, mutation: &Mutation) {
        // Stocker le nouveau code
        if !mutation.code_patch.is_empty() {
            self.code = mutation.code_patch.clone();
        }

        self.mutations_history.push(format!(
            "gen_{}: {:?} — {}",
            self.generation,
            mutation.mutation_type,
            mutation.description.chars().take(80).collect::<String>()
        ));

        self.generation += 1;
        // NOTE: fitness sera recalculée APRÈS passage au sandbox
    }

    /// Croisement : prend le meilleur code des deux parents
    pub fn crossover(parent_a: &Agent, parent_b: &Agent) -> Agent {
        let mut rng = rand::thread_rng();

        // Le parent avec la meilleure fitness donne son code
        let (code_donor, _trait_donor) = if parent_a.fitness >= parent_b.fitness {
            (parent_a, parent_b)
        } else {
            (parent_b, parent_a)
        };

        let mut child = Agent::new(if rng.gen_bool(0.5) {
            parent_a.role.clone()
        } else {
            parent_b.role.clone()
        });

        // Hériter le code du meilleur parent
        child.code = code_donor.code.clone();

        // Hériter les métriques du meilleur (sera recalculé au prochain sandbox)
        child.metrics = code_donor.metrics.clone();
        child.generation = parent_a.generation.max(parent_b.generation) + 1;
        child.calculate_fitness();
        child
    }

    pub fn is_weak(&self, threshold: f32) -> bool {
        self.fitness < threshold
    }

    pub fn is_stagnant(&self, threshold: usize) -> bool {
        self.stagnation_count >= threshold
    }
}

// ─────────────────────────────────────────────
// Mutations
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MutationType {
    Structural,
    Performance,
    Stability,
    Reasoning,
    MemoryCompression,
}

impl MutationType {
    pub fn random() -> Self {
        let types = [
            MutationType::Structural,
            MutationType::Performance,
            MutationType::Stability,
            MutationType::Reasoning,
            MutationType::MemoryCompression,
        ];
        let idx = rand::thread_rng().gen_range(0..types.len());
        types[idx].clone()
    }

    pub fn label(&self) -> &str {
        match self {
            MutationType::Structural => "structural",
            MutationType::Performance => "performance",
            MutationType::Stability => "stability",
            MutationType::Reasoning => "reasoning",
            MutationType::MemoryCompression => "memory_compression",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mutation {
    pub mutation_type: MutationType,
    pub code_patch: String,
    pub description: String,
}

// ─────────────────────────────────────────────
// Population
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Population {
    pub agents: Vec<Agent>,
    pub generation: u64,
    /// Historique de fitness moyenne par génération
    pub fitness_history: Vec<f32>,
}

impl Population {
    pub fn initialize(size: usize) -> Self {
        let roles = Role::all();
        let agents: Vec<Agent> = (0..size)
            .map(|i| {
                let role = roles[i % roles.len()].clone();
                Agent::new(role)
            })
            .collect();

        Self {
            agents,
            generation: 0,
            fitness_history: Vec::new(),
        }
    }

    pub fn average_fitness(&self) -> f32 {
        if self.agents.is_empty() {
            return 0.0;
        }
        let total: f32 = self.agents.iter().map(|a| a.fitness).sum();
        total / self.agents.len() as f32
    }

    pub fn best_agent(&self) -> Option<&Agent> {
        self.agents
            .iter()
            .max_by(|a, b| a.fitness.partial_cmp(&b.fitness).unwrap())
    }

    pub fn weak_agents(&self, threshold: f32) -> Vec<&Agent> {
        self.agents
            .iter()
            .filter(|a| a.is_weak(threshold))
            .collect()
    }

    /// Détecte la stagnation globale de la population
    pub fn is_stagnating(&self, window: usize, min_improvement: f32) -> bool {
        if self.fitness_history.len() < window {
            return false;
        }
        let recent = &self.fitness_history[self.fitness_history.len() - window..];
        let first = recent[0];
        let last = recent[recent.len() - 1];
        (last - first).abs() < min_improvement
    }

    /// Sélection naturelle avec reproduction
    pub fn natural_selection(&mut self, survival_rate: f32) {
        // Enregistrer la fitness moyenne
        self.fitness_history.push(self.average_fitness());

        self.agents
            .sort_by(|a, b| b.fitness.partial_cmp(&a.fitness).unwrap());

        let survivors = (self.agents.len() as f32 * survival_rate) as usize;
        let survivors = survivors.max(2);

        for i in survivors..self.agents.len() {
            self.agents[i].alive = false;
        }

        let mut new_agents = Vec::new();
        let living: Vec<Agent> = self.agents.iter().filter(|a| a.alive).cloned().collect();
        let target_size = self.agents.len();
        let mut rng = rand::thread_rng();

        while living.len() + new_agents.len() < target_size {
            let parent_a = &living[rng.gen_range(0..living.len())];
            let parent_b = &living[rng.gen_range(0..living.len())];
            let child = Agent::crossover(parent_a, parent_b);
            new_agents.push(child);
        }

        self.agents = living;
        self.agents.extend(new_agents);
        self.generation += 1;
    }
}
