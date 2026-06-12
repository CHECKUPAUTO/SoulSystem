#!/usr/bin/env python3
"""
Tool based on arXiv:2605.00056v1 — Conditional Entropy Analysis of Token Distributions.
This script computes and visualizes the probability distribution of tokens
and their conditional entropy in a sliding window, demonstrating key ideas
from the paper on token-level predictability.
"""

import argparse
import sys
from collections import Counter
from typing import List, Tuple
import math

def tokenize(text: str) -> List[str]:
    """Simple whitespace-based tokenization."""
    return text.strip().split()

def compute_token_probs(tokens: List[str]) -> List[Tuple[str, float]]:
    """Compute unigram probabilities for tokens."""
    counts = Counter(tokens)
    total = len(tokens)
    return [(tok, cnt / total) for tok, cnt in counts.items()]

def conditional_entropy(tokens: List[str], window_size: int = 2) -> float:
    """Estimate conditional entropy H(T_i | T_{i-1:i-window_size}) using sliding window."""
    if len(tokens) <= window_size:
        return 0.0
    
    # Build conditional counts: history -> [next_token counts]
    history_counts = {}
    for i in range(window_size, len(tokens)):
        history = tuple(tokens[i-window_size:i])
        next_tok = tokens[i]
        if history not in history_counts:
            history_counts[history] = Counter()
        history_counts[history][next_tok] += 1
    
    # Compute entropy
    total_cond_entropy = 0.0
    total_weight = 0
    for history, next_counts in history_counts.items():
        n = sum(next_counts.values())
        total_weight += n
        # Entropy for this history
        h = -sum((c/n) * math.log2(c/n) for c in next_counts.values() if c > 0)
        total_cond_entropy += h * n
    
    return total_cond_entropy / total_weight if total_weight > 0 else 0.0

def main():
    parser = argparse.ArgumentParser(
        description="Analyze token distribution and conditional entropy from text input."
    )
    parser.add_argument(
        "--window", type=int, default=2, help="Context window size for conditional entropy (default: 2)"
    )
    parser.add_argument(
        "--top-k", type=int, default=10, help="Number of top tokens to display (default: 10)"
    )
    args = parser.parse_args()

    # Read text from stdin or file
    text = sys.stdin.read()
    tokens = tokenize(text)
    if not tokens:
        print("No tokens found.", file=sys.stderr)
        sys.exit(1)

    # Compute probabilities
    probs = sorted(compute_token_probs(tokens), key=lambda x: -x[1])
    top_k = probs[:args.top_k]

    # Compute conditional entropy
    cond_ent = conditional_entropy(tokens, args.window)

    # Output
    print(f"Conditional Entropy (window={args.window}): {cond_ent:.4f} bits")
    print(f"\nTop {len(top_k)} tokens by probability:")
    for tok, p in top_k:
        print(f"  {tok}: {p:.4f}")

if __name__ == "__main__":
    main()