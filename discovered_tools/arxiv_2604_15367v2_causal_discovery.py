#!/usr/bin/env python3
"""
Causal Discovery Tool based on arXiv:2604.15367v2
Implements asymmetric regression for pairwise causal direction inference.
"""

import argparse
import numpy as np
from typing import Tuple, Optional


def asymmetric_regression_score(x: np.ndarray, y: np.ndarray) -> float:
    """
    Compute the asymmetric regression score to infer causal direction.
    Lower score for the true cause -> effect direction.
    """
    # Fit y ~ x and x ~ y using linear regression
    def fit_and_residuals(a: np.ndarray, b: np.ndarray) -> Tuple[np.ndarray, float]:
        a = a.reshape(-1, 1)
        coeffs = np.linalg.lstsq(a, b, rcond=None)[0]
        preds = a @ coeffs
        residuals = b - preds
        # Skewness of residuals (asymmetry indicator)
        mu = np.mean(residuals)
        sigma = np.std(residuals, ddof=1)
        if sigma == 0:
            return residuals, 0.0
        skew = np.mean(((residuals - mu) / sigma) ** 3)
        return residuals, np.abs(skew)

    _, skew_xy = fit_and_residuals(x, y)
    _, skew_yx = fit_and_residuals(y, x)
    return skew_xy - skew_yx


def infer_causal_direction(x: np.ndarray, y: np.ndarray, threshold: float = 0.0) -> str:
    """
    Infer causal direction: 'X->Y', 'Y->X', or 'indistinguishable'.
    """
    score = asymmetric_regression_score(x, y)
    if score < -threshold:
        return "X->Y"
    elif score > threshold:
        return "Y->X"
    else:
        return "indistinguishable"


def main():
    parser = argparse.ArgumentParser(
        description="Causal direction inference using asymmetric regression (arXiv:2604.15367v2)."
    )
    parser.add_argument("--data", nargs=2, type=str, required=True,
                        help="Two space-separated files containing X and Y data (one value per line).")
    parser.add_argument("--threshold", type=float, default=0.0,
                        help="Threshold for decision margin (default: 0.0).")
    args = parser.parse_args()

    # Load data
    x = np.loadtxt(args.data[0])
    y = np.loadtxt(args.data[1])
    if len(x) != len(y):
        raise ValueError("X and Y must have same length.")

    direction = infer_causal_direction(x, y, threshold=args.threshold)
    print(f"Inferred causal direction: {direction}")


if __name__ == "__main__":
    main()
