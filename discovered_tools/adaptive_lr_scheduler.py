#!/usr/bin/env python3
"""
Adaptive Learning Rate Scheduler

Based on the idea from arXiv:2605.00591v1: dynamic learning rate adjustment
using gradient variance estimation to stabilize and accelerate convergence.
"""

import argparse
import math
from typing import Callable, List, Tuple


def gradient_variance_estimator(
    params: List[float],
    loss_fn: Callable[[List[float]], float],
    eps: float = 1e-4
) -> float:
    """Estimate gradient variance using perturbation sampling."""
    n_params = len(params)
    grads_sq = []
    base_loss = loss_fn(params)
    for i in range(n_params):
        # Forward and backward perturbations
        params_plus = params.copy()
        params_plus[i] += eps
        loss_plus = loss_fn(params_plus)
        params_minus = params.copy()
        params_minus[i] -= eps
        loss_minus = loss_fn(params_minus)
        # Central difference gradient
        grad = (loss_plus - loss_minus) / (2 * eps)
        grads_sq.append(grad * grad)
    # Return average squared gradient magnitude as proxy for variance
    return sum(grads_sq) / n_params


def adaptive_lr_scheduler(
    init_lr: float = 0.1,
    decay_factor: float = 0.9,
    patience: int = 3,
    min_lr: float = 1e-6,
    max_lr: float = 1.0
) -> Tuple[Callable[[float], float], List[float]]:
    """Returns a closure that computes adaptive LR based on gradient variance."""
    lr = init_lr
    stable_count = 0
    prev_var = None

    def update(var: float) -> float:
        nonlocal lr, stable_count, prev_var
        if prev_var is not None:
            # If variance drops significantly, increase LR (accelerate)
            if var < prev_var * 0.7:
                lr = min(lr * 1.1, max_lr)
                stable_count = 0
            # If variance spikes, decrease LR (stabilize)
            elif var > prev_var * 1.5:
                lr = max(lr * decay_factor, min_lr)
                stable_count = 0
            else:
                stable_count += 1
                # Slow decay when stable
                if stable_count >= patience:
                    lr = max(lr * 0.99, min_lr)
                    stable_count = 0
        prev_var = var
        return lr

    return update, [init_lr]


def main():
    parser = argparse.ArgumentParser(
        description="Adaptive learning rate scheduler based on gradient variance."
    )
    parser.add_argument(
        "--init-lr", type=float, default=0.1, help="Initial learning rate"
    )
    parser.add_argument(
        "--decay-factor", type=float, default=0.9, help="Decay factor on high variance"
    )
    parser.add_argument(
        "--patience", type=int, default=3, help="Steps before slow decay"
    )
    parser.add_argument(
        "--steps", type=int, default=10, help="Number of simulation steps"
    )
    args = parser.parse_args()

    # Dummy loss function: L2 norm squared
    def loss_fn(params: List[float]) -> float:
        return sum(p * p for p in params)

    # Simulate parameter vector
    params = [1.0, -0.5, 0.2]

    update_lr, lrs = adaptive_lr_scheduler(
        init_lr=args.init_lr,
        decay_factor=args.decay_factor,
        patience=args.patience
    )

    print("Step | Variance | Learning Rate")
    print("-" * 35)
    for step in range(args.steps):
        var = gradient_variance_estimator(params, loss_fn)
        lr = update_lr(var)
        lrs.append(lr)
        # Simulate small update (not part of real training)
        params = [p - lr * (2 * p) * 0.01 for p in params]
        print(f"{step+1:4d} | {var:8.4f} | {lr:.6f}")


if __name__ == "__main__":
    main()
