#!/usr/bin/env python3
"""
Adaptive Weighted Ensemble for Drift-Aware Prediction.

This tool implements an adaptive ensemble method that dynamically reweights
base learners based on recent performance and distribution drift detection,
as described in arXiv:2605.00061v1.
"""

import argparse
import numpy as np
from typing import List, Tuple


def euclidean_distance(a: np.ndarray, b: np.ndarray) -> float:
    """Compute Euclidean distance between two points."""
    return float(np.linalg.norm(a - b))


def compute_ks_statistic(sample1: np.ndarray, sample2: np.ndarray) -> float:
    """Compute Kolmogorov-Smirnov statistic for drift detection."""
    sorted_data = np.sort(np.concatenate([sample1, sample2]))
    cdf1 = np.searchsorted(sorted_data, sample1, side='right') / len(sample1)
    cdf2 = np.searchsorted(sorted_data, sample2, side='right') / len(sample2)
    return float(np.max(np.abs(cdf1 - cdf2)))


class AdaptiveEnsemble:
    """Adaptive weighted ensemble with drift detection."""

    def __init__(self, n_learners: int = 5, drift_threshold: float = 0.2, decay: float = 0.9):
        self.n_learners = n_learners
        self.drift_threshold = drift_threshold
        self.decay = decay
        self.weights = np.ones(n_learners) / n_learners
        self.learner_history: List[List[float]] = [[] for _ in range(n_learners)]
        self.reference_window: List[np.ndarray] = []
        self.current_window: List[np.ndarray] = []

    def update(self, predictions: List[np.ndarray], targets: np.ndarray, features: np.ndarray):
        """Update ensemble weights and detect drift."""
        # Update learner histories
        for i, pred in enumerate(predictions):
            self.learner_history[i].append(float(np.mean(pred == targets)))

        # Maintain sliding windows for drift detection
        self.current_window.append(features)
        if len(self.current_window) > 50:
            self.current_window.pop(0)
        if len(self.current_window) == 50 and len(self.reference_window) == 0:
            self.reference_window = self.current_window.copy()
        elif len(self.current_window) == 50 and len(self.reference_window) == 50:
            # Compute drift score
            drift_scores = []
            for i in range(50):
                dist = euclidean_distance(
                    np.mean(self.reference_window[i], axis=0),
                    np.mean(self.current_window[i], axis=0)
                )
                drift_scores.append(dist)
            drift_score = np.mean(drift_scores)
            if drift_score > self.drift_threshold:
                # Reset reference window
                self.reference_window = self.current_window.copy()
                # Reset weights to uniform
                self.weights = np.ones(self.n_learners) / self.n_learners

n        # Update weights based on recent accuracy
        recent_accuracies = [np.mean(self.learner_history[i][-10:]) for i in range(self.n_learners)]
        exp_scores = np.exp(np.array(recent_accuracies) * 10)
        self.weights = exp_scores / exp_scores.sum()
        # Apply exponential decay to prevent overfitting to recent data
        self.weights = self.decay * self.weights + (1 - self.decay) / self.n_learners
        self.weights /= self.weights.sum()

    def predict(self, predictions: List[np.ndarray]) -> np.ndarray:
        """Predict using weighted ensemble."""
        stacked = np.stack(predictions, axis=0)  # shape: (n_learners, n_samples)
        weighted = np.average(stacked, axis=0, weights=self.weights)
        return (weighted > 0.5).astype(int)


def main():
    parser = argparse.ArgumentParser(description="Adaptive Weighted Ensemble")
    parser.add_argument('--n_samples', type=int, default=200, help='Number of samples')
    parser.add_argument('--n_features', type=int, default=4, help='Number of features')
    parser.add_argument('--n_learners', type=int, default=5, help='Number of base learners')
    args = parser.parse_args()

    np.random.seed(42)
    X = np.random.randn(args.n_samples, args.n_features)
    y = (X[:, 0] + X[:, 1] > 0).astype(int)

    # Simulate base learners
    learners = [np.random.rand(args.n_samples) for _ in range(args.n_learners)]
    ensemble = AdaptiveEnsemble(n_learners=args.n_learners)

    # Update ensemble
    ensemble.update(learners, y, X)

    # Predict
    pred = ensemble.predict(learners)
    accuracy = float(np.mean(pred == y))

    print(f"Ensemble weights: {ensemble.weights}")
    print(f"Accuracy: {accuracy:.3f}")


if __name__ == "__main__":
    main()
