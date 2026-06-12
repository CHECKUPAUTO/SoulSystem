#!/usr/bin/env python3
"""
ArXiv Paper Summarizer (inspired by arXiv:2605.00066v1)
Implements a lightweight extractive summarizer using sentence importance scoring
based on position, TF-IDF-like weighting, and sentence cohesion.
"""

import argparse
import re
import sys
from collections import Counter
from math import log10

def tokenize(text):
    """Simple tokenizer: lowercase, keep alphanumeric, split on whitespace."""
    return re.findall(r'[a-zA-Z0-9]+', text.lower())

def compute_idf(sentences):
    """Compute inverse document frequency for each term."""
    doc_freq = Counter()
    for sent in sentences:
        unique_words = set(tokenize(sent))
        for word in unique_words:
            doc_freq[word] += 1
    n = len(sentences) or 1
    return {word: log10(n / (1 + freq)) for word, freq in doc_freq.items()}

def score_sentence(sentence, idf, idx, total):
    """Score a sentence by position, TF-IDF, and length normalization."""
    tokens = tokenize(sentence)
    if not tokens:
        return 0.0
    # TF-IDF-like score
    tf = Counter(tokens)
    tfidf = sum(tf[word] * idf.get(word, 0) for word in tokens) / len(tokens)
    # Position bonus: first and last sentences often more important
    pos_score = 1.0 if idx == 0 else (0.7 if idx == total - 1 else 0.3)
    # Length penalty: avoid very long or very short sentences
    len_score = 1.0 if 5 <= len(tokens) <= 30 else 0.5
    return tfidf * pos_score * len_score

def extractive_summary(text, num_sentences=3):
    """Extract top-N most important sentences."""
    # Split into sentences (naive)
    raw_sentences = re.split(r'(?<=[.!?])\s+', text.strip())
    sentences = [s.strip() for s in raw_sentences if s.strip()]
    if len(sentences) <= num_sentences:
        return sentences
    idf = compute_idf(sentences)
    scored = [(score_sentence(s, idf, i, len(sentences)), i, s) for i, s in enumerate(sentences)]
    scored.sort(key=lambda x: (-x[0], x[1]))  # Higher score first, then original order
    top = sorted(scored[:num_sentences], key=lambda x: x[1])  # Restore order
    return [s for _, _, s in top]

def main():
    parser = argparse.ArgumentParser(description="Generate extractive summary of arXiv-style papers.")
    parser.add_argument("input_file", nargs="?", type=argparse.FileType("r"), default=sys.stdin,
                        help="Path to paper text (default: stdin)")
    parser.add_argument("-n", "--num-sentences", type=int, default=3, help="Number of summary sentences")
    args = parser.parse_args()
    
    text = args.input_file.read()
    summary = extractive_summary(text, args.num_sentences)
    print("\n".join(summary))

if __name__ == "__main__":
    main()