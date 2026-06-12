#!/usr/bin/env python3
"""
Adaptive Thresholding Tool - Sauvola-inspired binarization for degraded documents.

This tool implements a simplified version of Sauvola's adaptive thresholding
method, which computes local thresholds based on mean and standard deviation
within a sliding window, with a bias toward darker regions.

Reference: Sauvola, J., & Pietikäinen, M. (2000). Adaptive document image binarization.
"""

import argparse
import sys
import numpy as np
from PIL import Image


def sauvola_threshold(image: np.ndarray, window_size: int = 15, k: float = 0.2, r: float = 128.0) -> np.ndarray:
    """
    Apply Sauvola's adaptive thresholding to a grayscale image.
    
    Args:
        image: 2D numpy array (grayscale, 0-255).
        window_size: Size of the local window (must be odd).
        k: Weight factor for standard deviation (higher = more sensitive to variance).
        r: Normalization factor (typically 128 for 8-bit images).
    
    Returns:
        Binary image (0 or 255) as uint8 numpy array.
    """
    if window_size % 2 == 0:
        raise ValueError("window_size must be odd.")
    
    pad = window_size // 2
    padded = np.pad(image, pad, mode='reflect')
    h, w = image.shape
    binary = np.zeros_like(image, dtype=np.uint8)
    
    # Precompute integral images for speed
    integral = padded.cumsum(axis=0).cumsum(axis=1)
    
    # Compute local mean and std efficiently
    for i in range(h):
        for j in range(w):
            x1, y1 = i, j
            x2, y2 = i + 2 * pad, j + 2 * pad
            
            total = integral[x2, y2] - integral[x1, y2] - integral[x2, y1] + integral[x1, y1]
            count = window_size ** 2
            mean = total / count
            
            # Compute sum of squares via second integral image
            sq_padded = (padded ** 2).cumsum(axis=0).cumsum(axis=1)
            sq_total = sq_padded[x2, y2] - sq_padded[x1, y2] - sq_padded[x2, y1] + sq_padded[x1, y1]
            var = sq_total / count - mean ** 2
            std = np.sqrt(max(var, 0))
            
            threshold = mean * (1 + k * (std / r - 1))
            binary[i, j] = 255 if image[i, j] >= threshold else 0
    
    return binary


def main():
    parser = argparse.ArgumentParser(description="Binarize degraded document images using Sauvola's adaptive thresholding.")
    parser.add_argument("input", help="Path to input grayscale image (PNG/JPG)")
    parser.add_argument("output", help="Path to output binary image (PNG)")
    parser.add_argument("--window", type=int, default=15, help="Local window size (odd, default: 15)")
    parser.add_argument("--k", type=float, default=0.2, help="Sensitivity factor (default: 0.2)")
    parser.add_argument("--r", type=float, default=128.0, help="Normalization factor (default: 128)")
    
    args = parser.parse_args()
    
    try:
        img = Image.open(args.input).convert('L')
        arr = np.array(img, dtype=np.float64)
        binary = sauvola_threshold(arr, window_size=args.window, k=args.k, r=args.r)
        result = Image.fromarray(binary)
        result.save(args.output)
        print(f"Binarized image saved to {args.output}")
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
