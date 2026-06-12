#!/usr/bin/env python3
"""
Adaptive Learning Rate Tool

This tool demonstrates adaptive learning rate scheduling based on gradient variance,
inspired by methods like RMSProp and Adam. It simulates optimization of a simple
quadratic function and adjusts the learning rate per-parameter using running averages
of squared gradients.
"""

import argparse
import numpy as np


def f(x):
    """Quadratic objective function: f(x) = x^2."""
    return x ** 2


def grad_f(x):
    """Gradient of f(x) = x^2: f'(x) = 2x."""
    return 2 * x


def adaptive_sgd(x0, lr=0.1, beta=0.9, steps=100, seed=42):
    """
    Adaptive SGD with RMSProp-style learning rate adjustment.
    
    Args:
        x0: Initial parameter value.
        lr: Base learning rate.
        beta: Decay rate for running average of squared gradients.
        steps: Number of optimization steps.
        seed: Random seed for reproducibility.
    
    Returns:
        List of parameter values and losses at each step.
    """
    np.random.seed(seed)
    x = x0
    avg_sq_grad = 0.0
    eps = 1e-8
    history = []
    
    for _ in range(steps):
        g = grad_f(x)
        # Update running average of squared gradients
        avg_sq_grad = beta * avg_sq_grad + (1 - beta) * g ** 2
        # Adaptive learning rate: lr / sqrt(avg_sq_grad + eps)
        adaptive_lr = lr / (np.sqrt(avg_sq_grad) + eps)
        # Update parameter
        x = x - adaptive_lr * g
        # Record state
        history.append((x.copy() if isinstance(x, np.ndarray) else x, f(x)))
    
    return history


def main():
    parser = argparse.ArgumentParser(
        description="Demonstrate adaptive learning rate optimization."
    )
    parser.add_argument(
        "--init", type=float, default=5.0,
        help="Initial parameter value (default: 5.0)"
    )
    parser.add_argument(
        "--steps", type=int, default=50,
        help="Number of optimization steps (default: 50)"
    )
    parser.add_argument(
        "--lr", type=float, default=0.1,
        help="Base learning rate (default: 0.1)"
    )
    parser.add_argument(
        "--beta", type=float, default=0.9,
        help="Decay rate for gradient squared (default: 0.9)"
    )
    args = parser.parse_args()
    
    history = adaptive_sgd(
        x0=args.init,
        lr=args.lr,
        beta=args.beta,
        steps=args.steps
    )
    
    # Print final result and last few steps
    print(f"Initial x: {args.init:.4f}, final x: {history[-1][0]:.6f}")
    print(f"Initial loss: {f(args.init):.4f}, final loss: {history[-1][1]:.6f}")
    print("\nLast 5 steps (x, loss):")
    for x, loss in history[-5:]:
        print(f"  x={x:.6f}, loss={loss:.6f}")


if __name__ == "__main__":
    main()
