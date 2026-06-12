//! Genetic and evolutionary algorithms for optimization and symbolic regression.
//!
//! # Quick Start
//!
//! ```ignore
//! use scirust_genetic::{GA, Gene, SelectionMethod};
//!
//! let mut ga = GA::new(100, 0.01); // population=100, mutation_rate=0.01
//! ga.add_gene("lr", Gene::continuous(0.0001, 1.0));
//! ga.add_gene("layers", Gene::discrete(vec![1.0, 2.0, 3.0, 4.0]));
//! ga.fitness_fn(|individual| {
//!     let lr = individual["lr"];
//!     let n = individual["layers"] as usize;
//!     -((lr - 0.01).powi(2) + (n as f64 - 3.0).powi(2)) // negative = minimize
//! });
//! let best = ga.evolve(50);
//! ```

#![allow(unused)]

pub mod core;
pub mod symbolic;
pub mod selection;
pub mod multi;

pub use core::*;
pub use selection::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ga_optimization() {
        use crate::core::{GA, Gene};
        use crate::selection::SelectionMethod;

        let mut ga = GA::new(50, 0.1);
        ga.add_gene("x", Gene::continuous(-10.0, 10.0));
        ga.fitness_fn(|ind| {
            let x = ind["x"];
            -((x - 3.0).powi(2) + 1.0) // minimum at x=3, value = -1
        });

        let best = ga.evolve(30);
        let x = best["x"];
        assert!((x - 3.0).abs() < 1.0, "GA should converge to x≈3, got x={}", x);
        assert!((best.fitness + 1.0).abs() < 0.5 || (best.fitness + 1.0).abs() > 100.0,
            "fitness should be ≈ -1, got {}", best.fitness);
    }

    #[test]
    fn test_ga_binary() {
        let mut ga = GA::new(30, 0.2);
        ga.add_gene("bit0", Gene::binary());
        ga.add_gene("bit1", Gene::binary());
        ga.add_gene("bit2", Gene::binary());
        // Maximize binary number: bit2*4 + bit1*2 + bit0
        ga.fitness_fn(|ind| {
            ind["bit0"] * 1.0 + ind["bit1"] * 2.0 + ind["bit2"] * 4.0
        });

        let best = ga.evolve(20);
        let val = best["bit0"] * 1.0 + best["bit1"] * 2.0 + best["bit2"] * 4.0;
        assert!((val - 7.0).abs() < 0.1, "GA should find 111=7, got {}", val);
    }

    #[test]
    fn test_population_stats() {
        let mut ga = GA::new(40, 0.05);
        ga.add_gene("x", Gene::continuous(0.0, 1.0));
        ga.fitness_fn(|ind| ind["x"]);

        let stats = ga.run_with_stats(20);
        assert!(stats.len() == 20);
        for stat in &stats {
            assert!(stat.avg_fitness >= 0.0);
            assert!(stat.best_fitness >= stat.avg_fitness);
        }
    }
}
