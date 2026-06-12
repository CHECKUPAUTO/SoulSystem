//! Méta-apprentissage via OpenEvolve.
//!
//! Optimise périodiquement les hyperparamètres des modules via évolution.

use anyhow::Result;
use rand::Rng;

/// Trait pour les modules dont les hyperparamètres sont optimisables.
pub trait Optimizable {
    fn get_params(&self) -> Vec<f64>;
    fn set_params(&mut self, params: &[f64]);
    fn evaluate(&self) -> f64;
    fn name(&self) -> &str;
}

#[derive(Debug, Clone)]
pub struct EvolutionResult {
    pub best_params: Vec<f64>,
    pub best_score: f64,
    pub generations: u32,
}

pub async fn run_meta_evolution(
    module: &mut dyn Optimizable,
    generations: u32,
) -> Result<EvolutionResult> {
    let mut rng = rand::thread_rng();
    let pop_size = 20;
    let initial_params = module.get_params();
    let param_count = initial_params.len();

    let mut population: Vec<Vec<f64>> = (0..pop_size)
        .map(|_| {
            initial_params
                .iter()
                .map(|p| p + rng.gen_range(-0.5..0.5))
                .collect()
        })
        .collect();

    let mut best_score = f64::NEG_INFINITY;
    let mut best_params = initial_params.clone();

    for _gen in 0..generations {
        let scores: Vec<f64> = population
            .iter()
            .map(|p| {
                module.set_params(p);
                module.evaluate()
            })
            .collect();

        for (i, &score) in scores.iter().enumerate() {
            if score > best_score {
                best_score = score;
                best_params = population[i].clone();
            }
        }

        let mut next_gen = Vec::with_capacity(pop_size);
        let best_idx = scores
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        next_gen.push(population[best_idx].clone());

        while next_gen.len() < pop_size {
            let t1 = rng.gen_range(0..pop_size);
            let t2 = rng.gen_range(0..pop_size);
            let parent1 = if scores[t1] > scores[t2] {
                &population[t1]
            } else {
                &population[t2]
            };
            let t3 = rng.gen_range(0..pop_size);
            let t4 = rng.gen_range(0..pop_size);
            let parent2 = if scores[t3] > scores[t4] {
                &population[t3]
            } else {
                &population[t4]
            };

            let mut child: Vec<f64> = parent1
                .iter()
                .zip(parent2.iter())
                .map(|(a, b)| if rng.gen_bool(0.5) { *a } else { *b })
                .collect();

            if rng.gen_bool(0.1) {
                let idx = rng.gen_range(0..child.len());
                child[idx] += rng.gen_range(-0.2..0.2);
            }
            next_gen.push(child);
        }
        population = next_gen;
    }

    module.set_params(&best_params);
    Ok(EvolutionResult {
        best_params,
        best_score,
        generations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockModule {
        params: Vec<f64>,
        name: String,
    }

    impl Optimizable for MockModule {
        fn get_params(&self) -> Vec<f64> {
            self.params.clone()
        }
        fn set_params(&mut self, params: &[f64]) {
            self.params = params.to_vec();
        }
        fn evaluate(&self) -> f64 {
            self.params.iter().sum()
        }
        fn name(&self) -> &str {
            &self.name
        }
    }

    #[tokio::test]
    async fn test_meta_evolution() {
        let mut module = MockModule {
            params: vec![0.1, 0.2, 0.3],
            name: "test".into(),
        };
        let result = run_meta_evolution(&mut module, 10).await.unwrap();
        assert!(result.best_score >= 0.0);
        assert_eq!(result.best_params.len(), 3);
    }
}
