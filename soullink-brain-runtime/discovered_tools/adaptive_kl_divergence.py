#!/usr/bin/env python3
"""
Adaptive KL Divergence Estimator
Implements adaptive binning for Kullback-Leibler divergence estimation,
as described in arXiv:2604.23073v2.

This tool estimates D_KL(P || Q) from samples using a data-driven
binning strategy that adapts to local density variations.
"""

import argparse
import numpy as np
from typing import Tuple, List


def _find_optimal_bins(data: np.ndarray, min_bins: int = 3, max_bins: int = 50) -> np.ndarray:
    """Adaptively determine bin edges using variance-based splitting."""
    if len(data) < 2:
        return np.linspace(data[0], data[0] + 1, 3)
    
    bins = np.linspace(np.min(data), np.max(data), min_bins + 1)
    
    for _ in range(max_bins - min_bins):
        # Find the bin with highest variance
        variances = []
        for i in range(len(bins) - 1):
            mask = (data >= bins[i]) & (data < bins[i + 1])
            if i == len(bins) - 2:
                mask |= (data == bins[i + 1])
            if np.sum(mask) > 1:
                variances.append(np.var(data[mask]))
            else:
                variances.append(0)
        
        if max(variances) == 0:
            break
        
        split_idx = np.argmax(variances)
        mid = (bins[split_idx] + bins[split_idx + 1]) / 2
        bins = np.insert(bins, split_idx + 1, mid)
    
    return bins


def _compute_histograms(x: np.ndarray, y: np.ndarray, bins: np.ndarray) -> Tuple[np.ndarray, np.ndarray]:
    """Compute normalized histograms for samples x and y over given bins."""
    hist_x, _ = np.histogram(x, bins=bins, density=False)
    hist_y, _ = np.histogram(y, bins=bins, density=False)
    
    # Add small epsilon to avoid log(0)
    p = (hist_x + 1e-10) / (hist_x.sum() + 1e-10 * len(bins))
    q = (hist_y + 1e-10) / (hist_y.sum() + 1e-10 * len(bins))
    
    return p, q


def adaptive_kl_divergence(x: np.ndarray, y: np.ndarray) -> float:
    """
    Estimate KL divergence D_KL(P || Q) using adaptive binning.
    
    Args:
        x: Samples from distribution P
        y: Samples from distribution Q
    
    Returns:
        Estimated KL divergence
    """
    # Combine data to find optimal bins
    combined = np.concatenate([x, y])
    bins = _find_optimal_bins(combined)
    
    p, q = _compute_histograms(x, y, bins)
    
    # KL divergence: sum(p * log(p/q))
    kl = np.sum(p * np.log(p / q))
    return kl


def main():
    parser = argparse.ArgumentParser(
        description="Estimate KL divergence between two distributions using adaptive binning."
    )
    parser.add_argument("--size", type=int, default=1000, help="Number of samples per distribution")
    parser.add_argument("--seed", type=int, default=42, help="Random seed")
    args = parser.parse_args()
    
    np.random.seed(args.seed)
    
    # Generate synthetic distributions: P ~ N(0,1), Q ~ N(0.5, 1)
    p_samples = np.random.normal(0, 1, args.size)
    q_samples = np.random.normal(0.5, 1, args.size)
    
    kl_est = adaptive_kl_divergence(p_samples, q_samples)
    print(f"Estimated D_KL(P || Q) = {kl_est:.4f}")
    print(f"Theoretical value ≈ 0.125 (for N(0,1) vs N(0.5,1))")


if __name__ == "__main__":
    main()
