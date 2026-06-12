#!/usr/bin/env python3
"""
Adaptive Gradient Clipping (GCA)

This tool implements Gradient Clipping Adaptatif (GCA), a technique to stabilize
neural network training by adaptively clipping gradients based on the ratio of
parameter norms to gradient norms, as proposed in arXiv:2605.00184v1.
"""

import argparse
import numpy as np


def gca_clip(grads, params, eps=1e-6, clip_factor=10.0):
    """
    Apply Adaptive Gradient Clipping (GCA).

    Args:
        grads (list or np.ndarray): List or array of gradient tensors.
        params (list or np.ndarray): List or array of parameter tensors.
        eps (float): Small constant to avoid division by zero.
        clip_factor (float): Maximum allowed ratio of ||grad|| / ||param||.

    Returns:
        list: Clipped gradients with same shape as input grads.
    """
    if not isinstance(grads, list):
        grads = [grads]
    if not isinstance(params, list):
        params = [params]

    if len(grads) != len(params):
        raise ValueError("Number of gradients must match number of parameters.")

    clipped_grads = []
    for g, p in zip(grads, params):
        p_norm = np.linalg.norm(p)
        g_norm = np.linalg.norm(g)
        if p_norm > 0 and g_norm > 0:
            scale = min(1.0, clip_factor * p_norm / (g_norm + eps))
        else:
            scale = 1.0
        clipped_grads.append(g * scale)
    return clipped_grads if len(clipped_grads) > 1 else clipped_grads[0]


def main():
    parser = argparse.ArgumentParser(
        description="Adaptive Gradient Clipping (GCA) - arXiv:2605.00184v1"
    )
    parser.add_argument(
        "--grads",
        type=str,
        required=True,
        help="Comma-separated list of gradient magnitudes (e.g., '1.2,0.8,3.5')",
    )
    parser.add_argument(
        "--params",
        type=str,
        required=True,
        help="Comma-separated list of parameter magnitudes (e.g., '0.5,0.4,1.0')",
    )
    parser.add_argument(
        "--clip_factor",
        type=float,
        default=10.0,
        help="Clipping threshold factor (default: 10.0)",
    )
    parser.add_argument(
        "--eps",
        type=float,
        default=1e-6,
        help="Numerical stability constant (default: 1e-6)",
    )

    args = parser.parse_args()

    grads = np.array([float(x) for x in args.grads.split(",")])
    params = np.array([float(x) for x in args.params.split(",")])

    clipped = gca_clip(grads, params, eps=args.eps, clip_factor=args.clip_factor)

    print("Original gradients:", grads)
    print("Clipped gradients:", np.round(clipped, 6))
    print("Clipping ratios:", np.round(clipped / (grads + args.eps), 4))


if __name__ == "__main__":
    main()
