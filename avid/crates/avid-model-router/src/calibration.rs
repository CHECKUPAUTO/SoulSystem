//! Isotonic calibration of difficulty scores (UCCI-style, arXiv:2605.18796).
//!
//! The [`DifficultyModel`](crate::DifficultyModel) outputs a raw score in
//! `[0, 1]`, but that score is not necessarily a *calibrated* probability that a
//! strong model is needed. Fitting a monotonic (isotonic) mapping from raw
//! scores to the empirical "needed-strong" rate — via the Pool-Adjacent-
//! Violators algorithm on logged outcomes — turns the score into a calibrated
//! probability, so the routing threshold corresponds to a real error budget.
//!
//! The mapping is monotone non-decreasing by construction (a higher raw score
//! never maps to a lower calibrated probability) and deterministic, so the
//! routing decision stays reproducible.

use serde::{Deserialize, Serialize};

/// One pooled block of the fitted step function, covering `[lo, hi]` raw scores
/// with a single calibrated `value`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Block {
    lo: f64,
    hi: f64,
    value: f64,
}

/// A fitted isotonic (monotone non-decreasing) calibrator: raw score → calibrated
/// probability.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct IsotonicCalibrator {
    blocks: Vec<Block>,
}

impl IsotonicCalibrator {
    /// Fit a calibrator on `(raw_score, label)` pairs (`label` in `[0, 1]`,
    /// typically the realized "needed strong" outcome). Returns an identity
    /// (empty) calibrator when given no points.
    pub fn fit(points: &[(f64, f64)]) -> Self {
        // One contiguous run of pooled points during the PAV sweep.
        struct Acc {
            lo: f64,
            hi: f64,
            sum: f64,
            weight: f64,
        }

        if points.is_empty() {
            return Self::default();
        }
        let mut pts: Vec<(f64, f64)> = points.to_vec();
        pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        // Pool-Adjacent-Violators: push each point as a block, then merge any
        // adjacent pair where the earlier block's mean exceeds the later one's.
        let mut stack: Vec<Acc> = Vec::with_capacity(pts.len());
        for (x, y) in pts {
            stack.push(Acc {
                lo: x,
                hi: x,
                sum: y,
                weight: 1.0,
            });
            while stack.len() >= 2 {
                let n = stack.len();
                let top = stack[n - 1].sum / stack[n - 1].weight;
                let prev = stack[n - 2].sum / stack[n - 2].weight;
                if prev > top + 1e-12 {
                    let merged = stack.pop().expect("len >= 2");
                    let into = stack.last_mut().expect("len >= 1");
                    into.lo = into.lo.min(merged.lo);
                    into.hi = into.hi.max(merged.hi);
                    into.sum += merged.sum;
                    into.weight += merged.weight;
                } else {
                    break;
                }
            }
        }

        let blocks = stack
            .into_iter()
            .map(|a| Block {
                lo: a.lo,
                hi: a.hi,
                value: (a.sum / a.weight).clamp(0.0, 1.0),
            })
            .collect();
        Self { blocks }
    }

    /// Whether this calibrator was actually fitted (non-identity).
    pub fn is_fitted(&self) -> bool {
        !self.blocks.is_empty()
    }

    /// Map a raw score to its calibrated probability. An unfitted calibrator is
    /// the identity (clamped to `[0, 1]`).
    pub fn calibrate(&self, score: f64) -> f64 {
        if self.blocks.is_empty() {
            return score.clamp(0.0, 1.0);
        }
        // Blocks are sorted ascending and partition the score axis; return the
        // first block whose upper edge covers the score (clamping past the ends).
        for b in &self.blocks {
            if score <= b.hi {
                return b.value;
            }
        }
        self.blocks
            .last()
            .map_or_else(|| score.clamp(0.0, 1.0), |b| b.value)
    }

    /// Mean absolute calibration error of `points` under this mapping (lower is
    /// better) — handy for verifying that calibration improved.
    pub fn mean_abs_error(&self, points: &[(f64, f64)]) -> f64 {
        if points.is_empty() {
            return 0.0;
        }
        let total: f64 = points
            .iter()
            .map(|(x, y)| (self.calibrate(*x) - y).abs())
            .sum();
        total / points.len() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfitted_is_identity() {
        let c = IsotonicCalibrator::default();
        assert!(!c.is_fitted());
        assert!((c.calibrate(0.3) - 0.3).abs() < 1e-9);
        assert!((c.calibrate(1.5) - 1.0).abs() < 1e-9); // clamped
    }

    #[test]
    fn monotone_input_is_preserved() {
        let c = IsotonicCalibrator::fit(&[(0.1, 0.0), (0.5, 0.5), (0.9, 1.0)]);
        assert!((c.calibrate(0.1) - 0.0).abs() < 1e-9);
        assert!((c.calibrate(0.5) - 0.5).abs() < 1e-9);
        assert!((c.calibrate(0.9) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn pools_adjacent_violators() {
        // Labels 0,1,0 over increasing scores: the last two pool to 0.5.
        let c = IsotonicCalibrator::fit(&[(1.0, 0.0), (2.0, 1.0), (3.0, 0.0)]);
        assert!((c.calibrate(1.0) - 0.0).abs() < 1e-9);
        assert!((c.calibrate(2.0) - 0.5).abs() < 1e-9);
        assert!((c.calibrate(3.0) - 0.5).abs() < 1e-9);
        // Output is monotone non-decreasing.
        assert!(c.calibrate(1.0) <= c.calibrate(2.0));
        assert!(c.calibrate(2.0) <= c.calibrate(3.0));
    }

    #[test]
    fn calibration_reduces_error_on_miscalibrated_scores() {
        // Raw scores are systematically too low: a score of 0.2 actually means
        // "needed strong" ~80% of the time.
        let mut points: Vec<(f64, f64)> = Vec::new();
        for _ in 0..10 {
            points.push((0.2, 1.0));
            points.push((0.2, 1.0));
            points.push((0.2, 1.0));
            points.push((0.2, 0.0)); // 0.2 → 0.75 empirically
            points.push((0.8, 0.0));
            points.push((0.8, 0.0));
            points.push((0.8, 0.0));
            points.push((0.8, 1.0)); // 0.8 → 0.25 empirically (inverted!)
        }
        let raw_error: f64 =
            points.iter().map(|&(x, y)| (x - y).abs()).sum::<f64>() / points.len() as f64;
        let c = IsotonicCalibrator::fit(&points);
        let calibrated_error = c.mean_abs_error(&points);
        assert!(
            calibrated_error < raw_error,
            "calibrated {calibrated_error} should beat raw {raw_error}"
        );
    }

    #[test]
    fn round_trips_through_json() {
        let c = IsotonicCalibrator::fit(&[(0.1, 0.0), (0.9, 1.0)]);
        let s = serde_json::to_string(&c).unwrap();
        let back: IsotonicCalibrator = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }
}
