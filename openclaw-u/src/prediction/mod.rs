//! Prediction — Forecasting CPU/mémoire par régression linéaire

use tracing::info;

/// Prédiction basée sur l'historique des métriques
#[derive(Debug, Clone)]
pub struct Predictor {
    pub window: usize,
    history: Vec<MetricPoint>,
}

#[derive(Debug, Clone)]
struct MetricPoint {
    cpu: f32,
    mem: f32,
    disk: f32,
    #[allow(dead_code)]
    timestamp: f64,
}

impl Predictor {
    pub fn new(window: usize) -> Self {
        Self {
            window,
            history: Vec::with_capacity(window * 2),
        }
    }

    pub fn record(&mut self, cpu: f32, mem: f32, disk: f32) {
        self.history.push(MetricPoint {
            cpu,
            mem,
            disk,
            timestamp: chrono::Utc::now().timestamp() as f64,
        });

        if self.history.len() > self.window * 2 {
            self.history.remove(0);
        }
    }

    /// Régression linéaire simple: y = ax + b
    fn linear_regression(&self, values: &[f32]) -> Option<(f64, f64)> {
        let n = values.len() as f64;
        if n < 2.0 {
            return None;
        }

        let sum_x: f64 = (0..values.len()).map(|i| i as f64).sum();
        let sum_y: f64 = values.iter().map(|v| *v as f64).sum();
        let sum_xy: f64 = values
            .iter()
            .enumerate()
            .map(|(i, v)| i as f64 * *v as f64)
            .sum();
        let sum_x2: f64 = (0..values.len()).map(|i| (i as f64).powi(2)).sum();

        let denominator = n * sum_x2 - sum_x * sum_x;
        if denominator.abs() < 1e-10 {
            return None;
        }

        let a = (n * sum_xy - sum_x * sum_y) / denominator;
        let b = (sum_y - a * sum_x) / n;

        Some((a, b))
    }

    pub fn predict(&self, steps_ahead: usize) -> Option<Prediction> {
        if self.history.len() < 3 {
            return None;
        }

        let recent: Vec<_> = self
            .history
            .iter()
            .rev()
            .take(self.window)
            .rev()
            .cloned()
            .collect();

        let cpu_values: Vec<f32> = recent.iter().map(|m| m.cpu).collect();
        let mem_values: Vec<f32> = recent.iter().map(|m| m.mem).collect();
        let disk_values: Vec<f32> = recent.iter().map(|m| m.disk).collect();

        let (cpu_a, cpu_b) = self.linear_regression(&cpu_values)?;
        let (mem_a, mem_b) = self.linear_regression(&mem_values)?;
        let (disk_a, disk_b) = self.linear_regression(&disk_values)?;

        let x = self.window as f64 + steps_ahead as f64;

        let cpu_pred = (cpu_a * x + cpu_b).clamp(0.0, 100.0);
        let mem_pred = (mem_a * x + mem_b).clamp(0.0, 100.0);
        let disk_pred = (disk_a * x + disk_b).clamp(0.0, 100.0);

        let confidence = if self.history.len() >= self.window {
            0.7
        } else {
            0.4
        };

        info!(
            "🔮 PREDICT: dans {} cycles | CPU:{:.0}% MEM:{:.0}% DISK:{:.0}% | conf={:.2}",
            steps_ahead, cpu_pred, mem_pred, disk_pred, confidence
        );

        Some(Prediction {
            cpu: cpu_pred as f32,
            mem: mem_pred as f32,
            disk: disk_pred as f32,
            confidence,
            steps_ahead,
        })
    }

    /// Détecte si une alerte est imminente
    pub fn will_alert(
        &self,
        steps_ahead: usize,
        cpu_threshold: f32,
        mem_threshold: f32,
        disk_threshold: f32,
    ) -> Option<String> {
        let pred = self.predict(steps_ahead)?;

        if pred.confidence < 0.5 {
            return None;
        }

        if pred.cpu > cpu_threshold {
            return Some(format!(
                "ALERTE CPU imminente: {:.0}% dans {} cycles",
                pred.cpu, steps_ahead
            ));
        }
        if pred.mem > mem_threshold {
            return Some(format!(
                "ALERTE MEM imminente: {:.0}% dans {} cycles",
                pred.mem, steps_ahead
            ));
        }
        if pred.disk > disk_threshold {
            return Some(format!(
                "ALERTE DISK imminente: {:.0}% dans {} cycles",
                pred.disk, steps_ahead
            ));
        }

        None
    }
}

#[derive(Debug, Clone)]
pub struct Prediction {
    pub cpu: f32,
    pub mem: f32,
    pub disk: f32,
    pub confidence: f64,
    pub steps_ahead: usize,
}
