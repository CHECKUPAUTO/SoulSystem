#!/usr/bin/env python3
"""
Adaptive Shannon Entropy Estimator for Streaming Data.
Implements a sliding-window entropy estimator with dynamic binning based on data distribution.
This tool demonstrates a practical entropy estimation technique suitable for real-time monitoring.
"""

import argparse
import math
import sys
from collections import deque
from typing import List, Tuple


def estimate_entropy(values: List[float], num_bins: int = 10) -> float:
    """Compute Shannon entropy using histogram-based probability estimation."""
    if not values:
        return 0.0
    
    min_val, max_val = min(values), max(values)
    if min_val == max_val:
        return 0.0
    
    bin_width = (max_val - min_val) / num_bins
    counts = [0] * num_bins
    
    for v in values:
        idx = min(int((v - min_val) / bin_width), num_bins - 1)
        counts[idx] += 1
    
    total = len(values)
    entropy = 0.0
    for c in counts:
        if c > 0:
            p = c / total
            entropy -= p * math.log2(p)
    
    return entropy


def adaptive_entropy_stream(stream: List[float], window_size: int = 100, min_entropy: float = 0.1) -> List[Tuple[int, float]]:
    """Process streaming data with adaptive windowing and entropy tracking."""
    window = deque(maxlen=window_size)
    results = []
    
    for i, val in enumerate(stream):
        window.append(val)
        if len(window) >= window_size // 2:
            entropy = estimate_entropy(list(window))
            # Adaptive adjustment: increase resolution if entropy is low
            if entropy < min_entropy:
                entropy = estimate_entropy(list(window), num_bins=min(20, len(window)))
            results.append((i, entropy))
    
    return results


def main():
    parser = argparse.ArgumentParser(description="Compute adaptive Shannon entropy for streaming data.")
    parser.add_argument("--window", type=int, default=100, help="Sliding window size (default: 100)")
    parser.add_argument("--min-entropy", type=float, default=0.1, help="Minimum entropy threshold for resolution adjustment")
    parser.add_argument("--seed", type=int, default=42, help="Random seed for synthetic data")
    parser.add_argument("--num-samples", type=int, default=500, help="Number of synthetic samples to generate")
    
    args = parser.parse_args()
    
    # Generate synthetic data (simple periodic + noise)
    import random
    random.seed(args.seed)
    stream = [math.sin(i * 0.1) + random.gauss(0, 0.1) for i in range(args.num_samples)]
    
    results = adaptive_entropy_stream(stream, window_size=args.window, min_entropy=args.min_entropy)
    
    # Output summary
    if results:
        avg_entropy = sum(e for _, e in results) / len(results)
        print(f"Processed {len(stream)} samples. Average entropy: {avg_entropy:.4f}")
        print(f"Min entropy: {min(e for _, e in results):.4f}, Max entropy: {max(e for _, e in results):.4f}")
    else:
        print("No data processed.", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
