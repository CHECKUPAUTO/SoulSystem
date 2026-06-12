#!/usr/bin/env python3
"""
Adaptive Quantile Filter for Real-time Outlier Detection.

This tool implements a real-time outlier detection method that dynamically
adjusts quantile thresholds based on recent data distribution changes,
as described in arXiv:2605.00583v1.
"""

import argparse
import sys
from collections import deque
from typing import List, Optional
import numpy as np


def adaptive_quantile_filter(
    data_stream: List[float],
    window_size: int = 50,
    base_quantile: float = 0.95,
    adaptation_rate: float = 0.1,
    min_deviation: float = 1.5,
) -> List[bool]:
    """
    Detect outliers in a data stream using an adaptive quantile threshold.

    Args:
        data_stream: Input sequence of numerical observations.
        window_size: Size of sliding window for local quantile estimation.
        base_quantile: Base quantile level for initial threshold.
        adaptation_rate: Rate at which the quantile adapts to distribution shifts (0–1).
        min_deviation: Minimum deviation (in std units) to flag as outlier.

    Returns:
        List of booleans indicating whether each point is an outlier.
    """
    if not data_stream:
        return []

    window = deque(maxlen=window_size)
    quantile_history = []
    thresholds = []
    is_outlier = []

    for i, x in enumerate(data_stream):
        if len(window) >= window_size:
            # Compute local quantile and std
            arr = np.array(window)
            q = np.quantile(arr, base_quantile)
            std = np.std(arr)
            # Adapt quantile based on recent trend
            if quantile_history:
                trend = q - quantile_history[-1]
                q += adaptation_rate * trend
            threshold = q + min_deviation * std
        else:
            # Warm-up phase: use global stats
            arr = np.array(data_stream[: i + 1])
            q = np.quantile(arr, base_quantile)
            std = np.std(arr)
            threshold = q + min_deviation * std

        thresholds.append(threshold)
        quantile_history.append(q)
        window.append(x)
        is_outlier.append(x > threshold)

    return is_outlier


def main():
    parser = argparse.ArgumentParser(
        description="Detect outliers in a numeric time series using adaptive quantile filtering."
    )
    parser.add_argument(
        "--window", type=int, default=50, help="Sliding window size (default: 50)"
    )
    parser.add_argument(
        "--quantile", type=float, default=0.95, help="Base quantile level (default: 0.95)"
    )
    parser.add_argument(
        "--adapt-rate",
        type=float,
        default=0.1,
        help="Adaptation rate (0–1, default: 0.1)",
    )
    parser.add_argument(
        "--deviation",
        type=float,
        default=1.5,
        help="Min deviation (std units, default: 1.5)",
    )
    parser.add_argument(
        "--input",
        type=str,
        default="-",
        help="Input file path (default: stdin, one number per line)",
    )
    args = parser.parse_args()

    # Read input
    if args.input == "-":
        data = [float(line.strip()) for line in sys.stdin if line.strip()]
    else:
        with open(args.input) as f:
            data = [float(line.strip()) for line in f if line.strip()]

    if not data:
        print("Error: Empty input.", file=sys.stderr)
        sys.exit(1)

    outliers = adaptive_quantile_filter(
        data,
        window_size=args.window,
        base_quantile=args.quantile,
        adaptation_rate=args.adapt_rate,
        min_deviation=args.deviation,
    )

    # Output: line index and outlier flag
    for i, (val, is_out) in enumerate(zip(data, outliers)):
        print(f"{i}\t{val:.6f}\t{is_out}")


if __name__ == "__main__":
    main()
