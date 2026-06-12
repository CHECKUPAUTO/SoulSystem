use soul_telemetry::metrics::TelemetryHub;

const DEFAULT_TILE_SIZE: usize = 32;
const MAX_TILE_SIZE: usize = 128;
const MIN_TILE_SIZE: usize = 16;
const WORK_STEALING_STEP: u64 = 50;
const MIN_WORK_STEALING_THRESHOLD: u64 = 25;
const INITIAL_WORK_STEALING_THRESHOLD: u64 = 100;

#[derive(Debug, Clone, Copy)]
pub struct Genome {
    pub matrix_tile_size: usize,
    pub work_stealing_threshold: u64,
}

pub struct EvolutionaryForge {
    pub current_genome: Genome,
    best_score: f64,
    generation: u64,
}

impl Default for EvolutionaryForge {
    fn default() -> Self {
        Self::new()
    }
}

impl EvolutionaryForge {
    pub fn new() -> Self {
        Self {
            current_genome: Genome {
                matrix_tile_size: DEFAULT_TILE_SIZE,
                work_stealing_threshold: INITIAL_WORK_STEALING_THRESHOLD,
            },
            best_score: 0.0,
            generation: 0,
        }
    }

    /// Évalue le génome actuel via les métriques réelles du TelemetryHub et
    /// applique une mutation si le fitness a diminué.
    ///
    /// Le fitness est défini comme le ratio tâches exécutées / cycles totaux,
    /// ce qui mesure l'efficacité d'exécution du scheduler. Un fitness plus
    /// élevé signifie que le scheduler produit plus de travail par cycle.
    pub fn evaluate_and_mutate(&mut self, telemetry: &TelemetryHub) -> bool {
        // Agrégation des métriques réelles depuis le hub
        let (total_tasks, total_cycles) = telemetry.aggregate_metrics();

        // Fitness = tâches / cycles (avec garde division par zéro)
        let fitness = if total_cycles > 0 {
            total_tasks as f64 / total_cycles as f64
        } else {
            0.0
        };

        self.generation += 1;

        if fitness > self.best_score {
            self.best_score = fitness;
            false // Le génome est stable, pas de mutation immédiate
        } else {
            self.mutate_genome();
            true // Le génome a muté
        }
    }

    /// Mutation génétique avec diversification des paramètres.
    /// Utilise un pattern cyclique basé sur la génération pour éviter
    /// de rester bloqué dans un optimum local.
    fn mutate_genome(&mut self) {
        let gen = self.generation;
        // Cycle entre différentes stratégies de mutation
        match gen % 4 {
            0 => {
                self.current_genome.matrix_tile_size =
                    if self.current_genome.matrix_tile_size >= MAX_TILE_SIZE {
                        MIN_TILE_SIZE
                    } else {
                        self.current_genome.matrix_tile_size * 2
                    };
            }
            1 => {
                self.current_genome.work_stealing_threshold = self
                    .current_genome
                    .work_stealing_threshold
                    .saturating_sub(WORK_STEALING_STEP)
                    .max(MIN_WORK_STEALING_THRESHOLD);
            }
            2 => {
                self.current_genome.matrix_tile_size =
                    if self.current_genome.matrix_tile_size == DEFAULT_TILE_SIZE {
                        DEFAULT_TILE_SIZE * 2
                    } else {
                        DEFAULT_TILE_SIZE
                    };
            }
            _ => {
                self.current_genome.work_stealing_threshold += WORK_STEALING_STEP;
            }
        }
    }

    /// Retourne la génération courante (nombre d'évaluations effectuées).
    pub fn generation(&self) -> u64 {
        self.generation
    }
}
