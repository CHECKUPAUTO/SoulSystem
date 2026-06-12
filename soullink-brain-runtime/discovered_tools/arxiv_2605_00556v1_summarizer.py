#!/usr/bin/env python3
"""
ArXiv Paper Summarizer (v1.0)

Implements a simple extractive summarization approach for arXiv papers,
focusing on sentence importance scoring via TF-IDF and position bias.
Designed for quick prototyping without external ML models.
"""

import argparse
import re
import sys
from collections import Counter
from math import log
from typing import List, Tuple


def tokenize(text: str) -> List[str]:
    """Simple tokenizer: lowercase, keep alphanumeric tokens."""
    return re.findall(r'\b[a-z]+\b', text.lower())


def split_sentences(text: str) -> List[str]:
    """Split text into sentences using basic punctuation rules."""
    sentences = re.split(r'(?<=[.!?])\s+', text.strip())
    return [s.strip() for s in sentences if s.strip()]


def compute_tf(sentences: List[str]) -> List[Counter]:
    """Compute term frequency for each sentence."""
    return [Counter(tokenize(s)) for s in sentences]


def compute_idf(sentences: List[str]) -> Counter:
    """Compute inverse document frequency over all sentences."""
    doc_freq = Counter()
    n = len(sentences)
    for s in sentences:
        terms = set(tokenize(s))
        doc_freq.update(terms)
    return {term: log(n / (freq + 1) + 1) for term, freq in doc_freq.items()}


def score_sentence(sentence: str, tf: Counter, idf: Counter, position: int, n_sentences: int) -> float:
    """Score a sentence using TF-IDF + position bonus (first/last sentences weighted)."""
    tokens = tokenize(sentence)
    if not tokens:
        return 0.0
    tfidf = sum(tf.get(t, 0) * idf.get(t, 0) for t in tokens) / len(tokens)
    # Position bonus: first and last sentences get +0.2
    pos_bonus = 0.2 if position == 0 or position == n_sentences - 1 else 0.0
    return tfidf + pos_bonus


def extractive_summary(text: str, ratio: float = 0.3) -> str:
    """Generate extractive summary by selecting top-scoring sentences."""
    sentences = split_sentences(text)
    if not sentences:
        return ""
    n_select = max(1, int(len(sentences) * ratio))
    tf_list = compute_tf(sentences)
    idf = compute_idf(sentences)
    scores = [
        (i, score_sentence(s, tf_list[i], idf, i, len(sentences)))
        for i, s in enumerate(sentences)
    ]
    # Sort by score descending, then by original position for stability
    scores.sort(key=lambda x: (-x[1], x[0]))
    top_indices = sorted([i for i, _ in scores[:n_select]])
    return " ".join(sentences[i] for i in top_indices)


def main():
    parser = argparse.ArgumentParser(
        description="Generate extractive summary for arXiv paper abstracts or sections."
    )
    parser.add_argument("--input", type=str, required=True, help="Input text (e.g., abstract)")
    parser.add_argument("--ratio", type=float, default=0.3, help="Summary length ratio (0.1–0.5)")
    args = parser.parse_args()

    if not (0.1 <= args.ratio <= 0.5):
        sys.stderr.write("Error: ratio must be between 0.1 and 0.5\n")
        sys.exit(1)

    summary = extractive_summary(args.input, args.ratio)
    print(summary)


if __name__ == "__main__":
    main()
