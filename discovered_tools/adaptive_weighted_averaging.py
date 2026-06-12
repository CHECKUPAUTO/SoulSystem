#!/usr/bin/env python3
"""
Adaptive Weighted Averaging Tool

This tool implements an adaptive weighted averaging mechanism for model predictions,
inspired by recent research in ensemble learning and dynamic weighting strategies.
Weights are updated online based on prediction accuracy on recent samples.
"""

import argparse
import numpy as np
from typing import List, Tuple


def adaptive_weight_update(
    weights: np.ndarray,
    predictions: np.ndarray,
    targets: np.ndarray,
    learning_rate: float = 0.1,
    temperature: float = 1.0
) -> np.ndarray:
    """
    Update weights using a softmax-normalized improvement score.
    
    Args:
        weights: Current weights (shape: [n_models])
        predictions: Model predictions (shape: [n_samples, n_models])
        targets: Ground truth (shape: [n_samples])
        learning_rate: Step size for weight updates
        temperature: Softmax temperature for smoothing
    
    Returns:
        Updated weights (normalized, sum to 1)
    """
    # Compute per-model accuracy on current batch
    correct = (predictions == targets[:, None]).astype(float)
    accuracies = correct.mean(axis=0)
    
    # Compute improvement scores relative to current weighted average accuracy
    current_pred = np.average(predictions, axis=1, weights=weights)
    current_acc = (current_pred == targets).mean()
    scores = (accuracies - current_acc) * learning_rate
    
    # Apply softmax and renormalize
    new_weights = np.exp(scores / temperature)
    return new_weights / new_weights.sum()


def run_adaptive_averaging(
    models_predictions: List[np.ndarray],
    targets: np.ndarray,
    n_iterations: int = 10,
    learning_rate: float = 0.1
) -> Tuple[np.ndarray, List[np.ndarray]]:
    """
    Run adaptive weighted averaging over multiple iterations.
    
    Args:
        models_predictions: List of prediction arrays (one per model)
        targets: Ground truth labels
        n_iterations: Number of weight update iterations
        learning_rate: Learning rate for weight updates
    
    Returns:
        Final weights and list of weights per iteration
    """
    predictions = np.column_stack(models_predictions)
    n_models = len(models_predictions)
    weights = np.ones(n_models) / n_models
    history = [weights.copy()]
    
    for _ in range(n_iterations):
        weights = adaptive_weight_update(
            weights, predictions, targets, learning_rate
        )
        history.append(weights.copy())
    
    return weights, history


def main():
    parser = argparse.ArgumentParser(
        description="Adaptive Weighted Averaging for ensemble predictions"
    )
    parser.add_argument(
        "--predictions",
        nargs="+",
        type=float,
        required=True,
        help="Predictions from each model (space-separated arrays, e.g., 0.1 0.9 0.4)"
    )
    parser.add_argument(
        "--targets",
        nargs="+",
        type=int,
        required=True,
        help="Ground truth labels (space-separated integers)"
    )
    parser.add_argument(
        "--n-iter",
        type=int,
        default=10,
        help="Number of weight update iterations (default: 10)"
    )
    parser.add_argument(
        "--lr",
        type=float,
        default=0.1,
        help="Learning rate (default: 0.1)"
    )
    args = parser.parse_args()
    
    # Validate inputs
    if len(args.predictions) != len(args.targets):
        raise ValueError("Number of predictions must match number of targets.")
    
    # Parse into arrays
    predictions = np.array(args.predictions)
    targets = np.array(args.targets)
    
    # Run algorithm
    final_weights, history = run_adaptive_averaging(
        [predictions], targets, n_iterations=args.n_iter, learning_rate=args.lr
    )
    
    # Output results
    print(f"Final weights: {final_weights}")
    print(f"Convergence history: {history}")


if __name__ == "__main__":
    main()