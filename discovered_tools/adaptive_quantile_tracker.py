#!/usr/bin/env python3
"""
P² Quantile Tracker - Adaptive Real-time Quantile Estimation

This tool implements the P² (P-squared) algorithm for online quantile estimation
without storing data points, as described in arXiv:2503.14459v2.

Usage:
    python adaptive_quantile_tracker.py --quantile 0.9 --stream 1 2 3 4 5
"""

import argparse
import sys
from typing import List, Optional


class P2QuantileTracker:
    """Online quantile estimator using the P² algorithm."""

    def __init__(self, quantile: float = 0.5):
        if not (0.0 < quantile < 1.0):
            raise ValueError("Quantile must be in (0, 1)")
        self.quantile = quantile
        self.n = 0
        # Marker heights (q_i) and positions (n_i)
        self.q = [0.0] * 5
        self.n = [0] * 5
        # Desired marker positions
        self.n_prime = [0.0] * 5
        # Marker increments
        self.dn = [0.0] * 5
        # Initialized flag
        self.initialized = False

    def _initialize(self, x: float):
        """Initialize markers with first 5 values."""
        if not hasattr(self, '_buffer'):
            self._buffer: List[float] = []
        self._buffer.append(x)
        if len(self._buffer) == 5:
            self._buffer.sort()
            self.q = self._buffer[:]
            self.n = [0, 1, 2, 3, 4]
            self.n_prime = [0.0, self.quantile / 2, self.quantile, (1 + self.quantile) / 2, 1.0]
            self.dn = [0.0, self.quantile / 2, self.quantile, (1 + self.quantile) / 2, 1.0]
            self.initialized = True
            delattr(self, '_buffer')

    def update(self, x: float):
        """Update quantile estimate with new observation."""
        if not self.initialized:
            self._initialize(x)
            return

        self.n += 1

        # Find cell k such that q[k] < x <= q[k+1]
        k = 0
        for i in range(4):
            if x < self.q[i + 1]:
                k = i
                break
        else:
            k = 3

        # Increment positions of markers > k
        for i in range(k + 1, 5):
            self.n[i] += 1

        # Update desired positions
        for i in range(5):
            self.n_prime[i] += self.dn[i]

        # Adjust marker heights if needed
        for i in range(1, 4):
            d = self.n[i] - self.n_prime[i]
            if (d >= 1 and self.n[i + 1] - self.n[i] > 1) or \
               (d <= -1 and self.n[i - 1] - self.n[i] < -1):
                d_sign = 1 if d > 0 else -1
                # P² formula for adjustment
                q_new = self.q[i] + d_sign * (
                    (self.q[i + d_sign] - self.q[i]) / (self.n[i + d_sign] - self.n[i])
                )
                self.q[i] = q_new
                self.n[i] += d_sign
                self.n_prime[i] += d_sign

    def quantile_estimate(self) -> float:
        """Return current quantile estimate."""
        if not self.initialized:
            return self._buffer[len(self._buffer) // 2] if hasattr(self, '_buffer') else 0.0
        return self.q[2]


def main():
    parser = argparse.ArgumentParser(description="P² quantile tracker")
    parser.add_argument('--quantile', type=float, default=0.5,
                        help='Target quantile (e.g., 0.5 for median)')
    parser.add_argument('--stream', type=float, nargs='+', default=[],
                        help='Input data stream as space-separated numbers')
    args = parser.parse_args()

    tracker = P2QuantileTracker(quantile=args.quantile)

    # Process stream from arguments or stdin
    stream = args.stream
    if not stream:
        for line in sys.stdin:
            for token in line.strip().split():
                try:
                    stream.append(float(token))
                except ValueError:
                    continue

    for value in stream:
        tracker.update(value)

    print(f"Estimated {args.quantile:.2f}-quantile: {tracker.quantile_estimate():.6f}")


if __name__ == "__main__":
    main()
