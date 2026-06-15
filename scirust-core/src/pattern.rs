//! Pattern memory — stores and retrieves learned energy/expression patterns.
//!
//! Used for experience replay: records symbolic patterns with success/failure
//! labels and retrieves the most promising patterns for future inference.

use crate::symbolic::Expr;

/// A recorded pattern with its success count.
#[derive(Debug, Clone)]
struct PatternEntry {
    key: Expr,
    value: Expr,
    successes: usize,
    attempts: usize,
}

/// A memory bank of symbolic expression patterns with reinforcement tracking.
#[derive(Debug, Default)]
pub struct PatternMemory {
    entries: Vec<PatternEntry>,
}

impl PatternMemory {
    pub fn new() -> Self {
        Self {
            entries: Vec::with_capacity(1024),
        }
    }

    /// Record a pattern observation.
    /// `key` is the input expression, `value` is the output/target,
    /// `success` indicates whether the prediction was correct.
    pub fn record(&mut self, key: &Expr, value: &Expr, success: bool) {
        // Look for existing entry
        for entry in &mut self.entries {
            if entry.key == *key && entry.value == *value {
                entry.attempts += 1;
                if success {
                    entry.successes += 1;
                }
                return;
            }
        }
        // New entry
        self.entries.push(PatternEntry {
            key: key.clone(),
            value: value.clone(),
            successes: if success { 1 } else { 0 },
            attempts: 1,
        });
    }

    /// Retrieve the n best patterns, sorted by success rate (descending).
    /// Returns (key, value, success_rate) tuples.
    pub fn best_patterns(&self, n: usize) -> Vec<(Expr, Expr, f64)> {
        let mut sorted: Vec<&PatternEntry> = self.entries.iter().collect();
        sorted.sort_by(|a, b| {
            let rate_a = a.successes as f64 / a.attempts.max(1) as f64;
            let rate_b = b.successes as f64 / b.attempts.max(1) as f64;
            rate_b
                .partial_cmp(&rate_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted
            .into_iter()
            .take(n)
            .map(|e| {
                let rate = e.successes as f64 / e.attempts.max(1) as f64;
                (e.key.clone(), e.value.clone(), rate)
            })
            .collect()
    }

    /// Total number of stored patterns.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_retrieve() {
        let mut pm = PatternMemory::new();
        let k1 = Expr::Const(1.0);
        let v1 = Expr::Const(2.0);
        pm.record(&k1, &v1, true);
        pm.record(&k1, &v1, true);
        pm.record(&k1, &v1, false);
        let best = pm.best_patterns(5);
        assert_eq!(best.len(), 1);
        assert_eq!(best[0].2, 2.0 / 3.0); // 2/3 success rate
    }

    #[test]
    fn best_patterns_sorted() {
        let mut pm = PatternMemory::new();
        pm.record(&Expr::Const(1.0), &Expr::Const(1.0), true);
        pm.record(&Expr::Const(2.0), &Expr::Const(2.0), false);
        pm.record(&Expr::Const(2.0), &Expr::Const(2.0), true);
        let best = pm.best_patterns(2);
        assert_eq!(best.len(), 2);
        // Second pattern has 1/2=0.5 rate, first has 1/1=1.0
        assert!(best[0].2 > best[1].2);
    }
}
