use soul_telemetry::metrics::TelemetryHub;

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
                matrix_tile_size: 32,
                work_stealing_threshold: 100,
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
                // Doubler la taille de tile (puis revenir à 32 si trop grand)
                self.current_genome.matrix_tile_size =
                    if self.current_genome.matrix_tile_size >= 128 { 16 } else { self.current_genome.matrix_tile_size * 2 };
            }
            1 => {
                // Réduire le seuil de work-stealing
                self.current_genome.work_stealing_threshold =
                    self.current_genome.work_stealing_threshold.saturating_sub(50).max(25);
            }
            2 => {
                // Alterner tile size entre 32 et 64
                self.current_genome.matrix_tile_size = if self.current_genome.matrix_tile_size == 32 { 64 } else { 32 };
            }
            _ => {
                // Augmenter le seuil de work-stealing
                self.current_genome.work_stealing_threshold += 50;
            }
        }
    }

    /// Retourne la génération courante (nombre d'évaluations effectuées).
    pub fn generation(&self) -> u64 {
        self.generation
    }
}
