#!/usr/bin/env python3
"""
Adaptive Quantization Tool

This tool implements local variance-based adaptive quantization,
where quantization step size is adjusted per local region based on
signal variance to preserve fidelity in high-variability areas.

Reference: arXiv:2605.00631v1 (hypothesized adaptive compression technique).
"""

import argparse
import sys
import numpy as np


def quantize_adaptive(signal: np.ndarray, block_size: int = 8, base_levels: int = 256) -> np.ndarray:
    """
    Quantize signal using local variance-based step size.
    
    Args:
        signal: 1D input signal (float32/float64).
        block_size: Size of local analysis window.
        base_levels: Base number of quantization levels.
    
    Returns:
        Quantized signal (same shape as input).
    """
    if signal.ndim != 1:
        raise ValueError("Only 1D signals supported.")
    
    n = len(signal)
    padded = np.pad(signal, (0, block_size - n % block_size), mode='edge')
    blocks = padded[: (n // block_size + 1) * block_size].reshape(-1, block_size)
    
    # Compute local variance per block
    variances = np.var(blocks, axis=1, ddof=0)
    # Avoid division by zero: clamp minimum variance
    variances = np.maximum(variances, 1e-12)
    
    # Step size inversely proportional to sqrt(variance) to maintain SNR
    base_step = (np.max(signal) - np.min(signal)) / base_levels
    steps = base_step * np.sqrt(np.mean(variances)) / np.sqrt(variances)
    steps = np.clip(steps, base_step / 10, base_step * 5)  # practical bounds
    
    # Quantize each block with its own step
    quantized = np.empty_like(padded)
    for i, (block, step) in enumerate(zip(blocks, steps)):
        center = np.mean(block)
        quantized[i * block_size : (i + 1) * block_size] = (
            np.round((block - center) / step) * step + center
        )
    
    return quantized[:n]


def main():
    parser = argparse.ArgumentParser(
        description="Apply adaptive quantization to a 1D signal (e.g., audio, sensor data)."
    )
    parser.add_argument("input_file", nargs="?", help="Input file (one float per line). If omitted, generates synthetic signal.")
    parser.add_argument("--block-size", type=int, default=8, help="Local analysis window size (default: 8)")
    parser.add_argument("--levels", type=int, default=256, help="Base quantization levels (default: 256)")
    parser.add_argument("--output", type=str, help="Output file (default: stdout)")
    
    args = parser.parse_args()
    
    if args.input_file:
        with open(args.input_file, "r") as f:
            signal = np.array([float(line.strip()) for line in f if line.strip()])
    else:
        # Generate synthetic signal: piecewise with varying variance
        t = np.linspace(0, 4 * np.pi, 1000)
        signal = np.sin(t) + 0.1 * np.random.randn(1000)
        signal[500:] += 0.5 * np.sin(3 * t[500:])  # higher variance region
    
    quantized = quantize_adaptive(signal, block_size=args.block_size, base_levels=args.levels)
    
    output_lines = [f"{x:.6f}" for x in quantized]
    output = "\n".join(output_lines)
    
    if args.output:
        with open(args.output, "w") as f:
            f.write(output + "\n")
    else:
        print(output)


if __name__ == "__main__":
    main()
