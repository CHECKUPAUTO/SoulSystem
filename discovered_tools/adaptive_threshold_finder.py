#!/usr/bin/env python3
"""
Adaptive Threshold Finder - Estimation adaptative de seuil par variance locale.
Basé sur l'idée d'optimisation locale du papier arXiv:2605.00742v1.
"""

import argparse
import sys
from typing import Tuple, List
import numpy as np


def local_variance(image: np.ndarray, window_size: int = 3) -> np.ndarray:
    """Compute local variance using sliding window."""
    pad = window_size // 2
    padded = np.pad(image, pad, mode='reflect')
    h, w = image.shape
    var_map = np.zeros_like(image, dtype=np.float64)
    
    for i in range(h):
        for j in range(w):
            window = padded[i:i+window_size, j:j+window_size]
            var_map[i, j] = np.var(window.astype(np.float64))
    return var_map


def adaptive_threshold(image: np.ndarray, k: float = 0.2, window_size: int = 3) -> np.ndarray:
    """
    Compute adaptive threshold using local mean and variance.
    Threshold = local_mean + k * local_std
    """
    if image.ndim == 3:
        image = np.mean(image, axis=2)
    image = image.astype(np.float64)
    
    pad = window_size // 2
    padded = np.pad(image, pad, mode='reflect')
    h, w = image.shape
    threshold_map = np.zeros_like(image)
    
    for i in range(h):
        for j in range(w):
            window = padded[i:i+window_size, j:j+window_size]
            local_mean = np.mean(window)
            local_std = np.std(window)
            threshold_map[i, j] = local_mean + k * local_std
    
    return threshold_map


def binarize(image: np.ndarray, threshold_map: np.ndarray) -> np.ndarray:
    """Apply adaptive threshold to produce binary image."""
    return (image >= threshold_map).astype(np.uint8) * 255


def main():
    parser = argparse.ArgumentParser(description="Adaptive thresholding via local variance optimization")
    parser.add_argument("input", nargs="?", default="-", help="Input image path (or '-' for stdin raw grayscale 8-bit)" )
    parser.add_argument("-k", type=float, default=0.2, help="Sensitivity factor (default: 0.2)")
    parser.add_argument("-w", "--window", type=int, default=3, help="Window size (default: 3)")
    parser.add_argument("-o", "--output", default="output.png", help="Output image path (default: output.png)")
    
    args = parser.parse_args()
    
    try:
        if args.input == "-":
            # Read raw grayscale 8-bit image from stdin (assumed 256x256)
            raw = sys.stdin.buffer.read()
            if len(raw) != 256*256:
                raise ValueError("stdin must be exactly 65536 bytes (256x256 grayscale)")
            image = np.frombuffer(raw, dtype=np.uint8).reshape((256, 256))
        else:
            import imageio.v2 as imageio
            image = imageio.imread(args.input)
            if image.ndim == 3 and image.shape[2] == 4:
                image = image[:, :, :3]
    except Exception as e:
        print(f"Error loading image: {e}", file=sys.stderr)
        sys.exit(1)
    
    if image.ndim == 3:
        image = np.mean(image, axis=2)
    
    threshold_map = adaptive_threshold(image, k=args.k, window_size=args.window)
    binary = binarize(image, threshold_map)
    
    try:
        import imageio.v2 as imageio
        imageio.imwrite(args.output, binary)
        print(f"Binary image saved to {args.output}")
    except Exception as e:
        print(f"Error saving output: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
