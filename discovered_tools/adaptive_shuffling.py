#!/usr/bin/env python3
"""
Adaptive Shuffling for Stochastic Optimization

This tool implements a simple adaptive shuffling strategy that dynamically
adjusts the shuffling frequency based on gradient variance estimates to
accelerate convergence in stochastic optimization.

Reference: arXiv:2604.19652v2 (hypothetical implementation based on adaptive shuffling principles).
"""

import argparse
import numpy as np
import sys

def generate_synthetic_data(n_samples=1000, n_features=10, noise=0.1):
    """Generate synthetic regression data."""
    np.random.seed(42)
    X = np.random.randn(n_samples, n_features)
    w_true = np.random.randn(n_features)
    y = X @ w_true + noise * np.random.randn(n_samples)
    return X, y, w_true

def compute_gradient(X_batch, y_batch, w):
    """Compute gradient of MSE loss."""
    residuals = X_batch @ w - y_batch
    return (2 / len(y_batch)) * (X_batch.T @ residuals)

def adaptive_shuffling_sgd(X, y, w0, epochs=50, batch_size=32, base_shuffle_freq=10, tolerance=1e-4):
    """
    Run SGD with adaptive shuffling based on gradient variance estimation.
    
    Shuffles more frequently when gradient variance is high, less when stable.
    """
    n_samples = len(y)
    w = w0.copy()
    var_history = []
    loss_history = []
    
    for epoch in range(epochs):
        # Estimate gradient variance over last few batches
        if epoch > 2:
            grads = []
            for _ in range(5):
                idx = np.random.choice(n_samples, batch_size, replace=False)
                grads.append(compute_gradient(X[idx], y[idx], w))
            grad_var = np.mean([np.linalg.norm(g)**2 for g in grads])
            var_history.append(grad_var)
            
            # Adjust shuffle frequency inversely to gradient variance
            shuffle_freq = max(1, int(base_shuffle_freq * (10 / (grad_var + 1e-6))))
        else:
            shuffle_freq = base_shuffle_freq
        
        # Shuffle data every `shuffle_freq` epochs
        if epoch % shuffle_freq == 0:
            perm = np.random.permutation(n_samples)
            X_shuf = X[perm]
            y_shuf = y[perm]
        
        # SGD step
        for i in range(0, n_samples, batch_size):
            X_batch = X_shuf[i:i+batch_size]
            y_batch = y_shuf[i:i+batch_size]
            if len(X_batch) == 0:
                continue
            grad = compute_gradient(X_batch, y_batch, w)
            lr = 0.01 / (1 + epoch * 0.005)  # decaying learning rate
            w -= lr * grad
        
        loss_history.append(np.mean((X @ w - y)**2))
        if len(loss_history) > 1 and abs(loss_history[-2] - loss_history[-1]) < tolerance:
            break
    
    return w, loss_history

def main():
    parser = argparse.ArgumentParser(description="Adaptive shuffling for stochastic optimization")
    parser.add_argument('--epochs', type=int, default=50, help='Number of epochs')
    parser.add_argument('--batch_size', type=int, default=32, help='Batch size')
    parser.add_argument('--n_samples', type=int, default=1000, help='Number of samples')
    parser.add_argument('--n_features', type=int, default=10, help='Number of features')
    args = parser.parse_args()
    
    X, y, w_true = generate_synthetic_data(args.n_samples, args.n_features)
    w0 = np.zeros(X.shape[1])
    
    w_opt, losses = adaptive_shuffling_sgd(X, y, w0, epochs=args.epochs, batch_size=args.batch_size)
    
    print(f"Final loss: {losses[-1]:.6f}")
    print(f"True weights norm: {np.linalg.norm(w_true):.4f}")
    print(f"Estimated weights norm: {np.linalg.norm(w_opt):.4f}")
    print(f"Weight error: {np.linalg.norm(w_true - w_opt):.6f}")

if __name__ == "__main__":
    main()
