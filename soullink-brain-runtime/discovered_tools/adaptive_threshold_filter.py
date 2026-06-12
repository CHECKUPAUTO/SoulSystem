#!/usr/bin/env python3
"""
Adaptive Threshold Filter

Implements a local adaptive thresholding technique for image segmentation,
inspired by the bias-correction approach in arXiv:2605.00384v1.

Usage:
    python adaptive_threshold_filter.py input.png output.png
"""

import argparse
import sys
import numpy as np
from PIL import Image


def local_mean(image: np.ndarray, window_size: int = 15) -> np.ndarray:
    """Compute local mean using a simple box filter (convolution with uniform kernel)."""
    pad = window_size // 2
    padded = np.pad(image, pad, mode='reflect')
    kernel = np.ones((window_size, window_size)) / (window_size ** 2)
    result = np.zeros_like(image, dtype=np.float64)
    for i in range(image.shape[0]):
        for j in range(image.shape[1]):
            result[i, j] = np.sum(padded[i:i+window_size, j:j+window_size] * kernel)
    return result


def adaptive_threshold(image: np.ndarray, k: float = 0.2, window_size: int = 15) -> np.ndarray:
    """
    Apply adaptive thresholding using local mean and standard deviation.
    
    Pixels are set to 1 if: pixel > local_mean - k * local_std
    Otherwise set to 0.
    """
    img = np.asarray(image, dtype=np.float64)
    if img.ndim == 3:
        img = img.mean(axis=2)  # Convert to grayscale
    
    local_mean_img = local_mean(img, window_size)
    
    # Compute local variance efficiently using same padding trick
    pad = window_size // 2
    padded_sq = np.pad(img ** 2, pad, mode='reflect')
    padded_img = np.pad(img, pad, mode='reflect')
    
    local_var = np.zeros_like(img)
    for i in range(img.shape[0]):
        for j in range(img.shape[1]):
            window = padded_img[i:i+window_size, j:j+window_size]
            window_sq = padded_sq[i:i+window_size, j:j+window_size]
            local_var[i, j] = np.mean(window_sq) - np.mean(window) ** 2
    
    local_std = np.sqrt(np.maximum(local_var, 0))
    
    threshold = local_mean_img - k * local_std
    binary = (img > threshold).astype(np.uint8) * 255
    return binary


def main():
    parser = argparse.ArgumentParser(description="Adaptive thresholding for image segmentation")
    parser.add_argument("input", help="Input image path")
    parser.add_argument("output", help="Output binary image path")
    parser.add_argument("--k", type=float, default=0.2,
                        help="Bias correction factor (default: 0.2)")
    parser.add_argument("--window", type=int, default=15,
                        help="Window size for local statistics (default: 15)")
    args = parser.parse_args()
    
    try:
        img = Image.open(args.input).convert('L')
    except Exception as e:
        print(f"Error loading image: {e}", file=sys.stderr)
        sys.exit(1)
    
    result = adaptive_threshold(img, k=args.k, window_size=args.window)
    output_img = Image.fromarray(result)
    output_img.save(args.output)
    print(f"Segmented image saved to {args.output}")


if __name__ == "__main__":
    main()
