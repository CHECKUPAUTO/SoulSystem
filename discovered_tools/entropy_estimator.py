#!/usr/bin/env python3
"""
Entropy estimator using the LZ76-based approach from arXiv:2605.00126v1.
This script computes an estimate of the Shannon entropy rate of a text stream
using the Lempel-Ziv parsing length, as proposed in the paper.
"""

import argparse
import sys
import math
from collections import defaultdict


def lz76_parse(text: str) -> list:
    """Parse text using LZ76 algorithm, returning list of phrases."""
    phrases = []
    i = 0
    while i < len(text):
        # Find longest match in already seen phrases
        best_len = 0
        best_end = i
        for j in range(i):
            length = 0
            while (i + length < len(text) and
                   j + length < i and
                   text[i + length] == text[j + length]):
                length += 1
            if length > best_len:
                best_len = length
                best_end = j
        # Extend by one character
        phrases.append(text[i:i + best_len + 1])
        i += best_len + 1
    return phrases


def estimate_entropy(text: str, block_size: int = 1) -> float:
    """
    Estimate Shannon entropy rate (bits per symbol) using LZ76 parsing.
    Implements the estimator H = log2(|Σ|) * (n / L) where L is LZ parsing length.
    For simplicity, uses binary alphabet assumption if text is binary-like,
    otherwise assumes ASCII (256 symbols).
    """
    if not text:
        return 0.0
    
    # Use LZ76 parsing
    phrases = lz76_parse(text)
    n = len(text)  # total symbols
    L = len(phrases)  # number of phrases
    
    if L == 0:
        return 0.0
    
    # Alphabet size (use 256 for byte-like, or 2 for binary)
    alphabet_size = 2 if all(c in '01' for c in text) else 256
    
    # Entropy rate estimate: H = log2(|Σ|) * n / L
    entropy = math.log2(alphabet_size) * n / L
    return min(entropy, math.log2(alphabet_size))  # cap at max entropy


def main():
    parser = argparse.ArgumentParser(
        description="Estimate Shannon entropy rate of input text using LZ76 parsing."
    )
    parser.add_argument(
        "-i", "--input", type=str, default="-",
        help="Input file path (default: stdin)"
    )
    parser.add_argument(
        "--binary", action="store_true",
        help="Assume binary alphabet (0/1) instead of byte (256 symbols)"
    )
    args = parser.parse_args()
    
    # Read input
    if args.input == "-":
        text = sys.stdin.read()
    else:
        with open(args.input, "r", encoding="utf-8", errors="ignore") as f:
            text = f.read()
    
    # Estimate entropy
    alphabet_size = 2 if args.binary else 256
    entropy = estimate_entropy(text)
    
    print(f"Alphabet size: {alphabet_size}")
    print(f"LZ76 phrases: {len(lz76_parse(text))}")
    print(f"Entropy rate estimate: {entropy:.4f} bits/symbol")
    print(f"Max possible entropy: {math.log2(alphabet_size):.4f} bits/symbol")


if __name__ == "__main__":
    main()
