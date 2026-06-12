//! Détecteur d'anomalies pour le HNN Mesh et OpenEvolve.
//!
//! Surveille les métriques et publie des alertes sur le bus
//! en cas d'anomalie détectée.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Configuration du détecteur d'anomalies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyConfig {
    /// Seuil de chute des ticks/sec (fraction, ex: 0.5 = 50% drop)
    pub tick_drop_threshold: f64,
    /// Fenêtre d'observation (nombre de mesures)
    pub window_size: usize,
    /// Seuil de divergence OpenEvolve (nb générations consécutives en baisse)
    pub evolve_divergence_threshold: u32,
}

impl Default for AnomalyConfig {
    fn default() -> Self {
        Self {
            tick_drop_threshold: 0.3,
            window_size: 10,
            evolve_divergence_threshold: 10,
        }
    }
}

/// Détecteur d'anomalies.
pub struct AnomalyDetector {
    config: AnomalyConfig,
    tick_history: VecDeque<u64>,
    evolve_scores: VecDeque<f64>,
    consecutive_declines: u32,
}

#[derive(Debug, Clone)]
pub enum Anomaly {
    HnnTickDrop {
        current: u64,
        previous_avg: f64,
        drop_percent: f64,
    },
    EvolveDivergence {
        crate_name: String,
        generations_declining: u32,
    },
}

impl AnomalyDetector {
    pub fn new(config: AnomalyConfig) -> Self {
        Self {
            config,
            tick_history: VecDeque::new(),
            evolve_scores: VecDeque::new(),
            consecutive_declines: 0,
        }
    }

    /// Enregistre un nouveau relevé de ticks/sec.
    pub fn record_ticks(&mut self, ticks: u64) -> Option<Anomaly> {
        self.tick_history.push_back(ticks);
        if self.tick_history.len() > self.config.window_size {
            self.tick_history.pop_front();
        }

        if self.tick_history.len() >= 2 {
            let prev_avg: f64 = self.tick_history.iter().take(self.tick_history.len() - 1)
                .map(|&t| t as f64).sum::<f64>() / (self.tick_history.len() - 1) as f64;

            if prev_avg > 0.0 {
                let drop_percent = 1.0 - (ticks as f64 / prev_avg);
                if drop_percent > self.config.tick_drop_threshold {
                    return Some(Anomaly::HnnTickDrop {
                        current: ticks,
                        previous_avg: prev_avg,
                        drop_percent,
                    });
                }
            }
        }
        None
    }

    /// Enregistre un score d'optimisation OpenEvolve.
    pub fn record_evolve_score(&mut self, crate_name: &str, score: f64) -> Option<Anomaly> {
        self.evolve_scores.push_back(score);

        if self.evolve_scores.len() >= 2 {
            let last = self.evolve_scores[self.evolve_scores.len() - 1];
            let prev = self.evolve_scores[self.evolve_scores.len() - 2];

            if last < prev {
                self.consecutive_declines += 1;
            } else {
                self.consecutive_declines = 0;
            }

            if self.consecutive_declines >= self.config.evolve_divergence_threshold {
                self.consecutive_declines = 0;
                self.evolve_scores.clear();
                return Some(Anomaly::EvolveDivergence {
                    crate_name: crate_name.to_string(),
                    generations_declining: self.config.evolve_divergence_threshold,
                });
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tick_drop_detection() {
        let mut detector = AnomalyDetector::new(AnomalyConfig::default());

        // Stable ticks
        for _ in 0..5 {
            assert!(detector.record_ticks(100_000).is_none());
        }

        // Sudden drop
        let anomaly = detector.record_ticks(30_000);
        assert!(anomaly.is_some());
        match anomaly.unwrap() {
            Anomaly::HnnTickDrop { current, .. } => assert_eq!(current, 30_000),
            _ => panic!("Wrong anomaly type"),
        }
    }

    #[test]
    fn test_evolve_divergence() {
        let mut detector = AnomalyDetector::new(AnomalyConfig {
            evolve_divergence_threshold: 5,
            ..Default::default()
        });

        // Declining scores
        for i in 0..6 {
            let score = 10.0 - i as f64;
            let result = detector.record_evolve_score("test_crate", score);
            if i == 5 {
                assert!(result.is_some());
            } else {
                assert!(result.is_none());
            }
        }
    }
}
