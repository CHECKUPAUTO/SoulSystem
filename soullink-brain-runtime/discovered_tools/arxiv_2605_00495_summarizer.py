#!/usr/bin/env python3
"""Extractive summarizer inspired by recent arXiv paper 2605.00495v1.

This tool implements a simple but effective sentence scoring method based on
TF-IDF weighting, sentence position, and keyword density to produce a concise
summary of text documents.
"""

import argparse
import re
import sys
from collections import Counter
from math import log10
from typing import List, Tuple


def tokenize(text: str) -> List[str]:
    """Lowercase and split text into words, keeping only alphabetic tokens."""
    return re.findall(r'[a-z]+', text.lower())


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
    total_docs = len(sentences)
    for sent in sentences:
        unique_words = set(tokenize(sent))
        for word in unique_words:
            doc_freq[word] += 1
    return Counter({word: log10(total_docs / (freq + 1e-9)) + 1 for word, freq in doc_freq.items()})


def score_sentence(sentence: str, tf_list: List[Counter], idf: Counter, position: int, total: int) -> float:
    """Score a sentence using TF-IDF, position bias, and keyword density."""
    tokens = tokenize(sentence)
    if not tokens:
        return 0.0
    
    # TF-IDF score
    tfidf = sum(tf_list[position].get(word, 0) * idf.get(word, 0) for word in tokens)
    
    # Position bias: first and last sentences are more important
    pos_score = 1.0 if position == 0 else (0.8 if position == total - 1 else 0.5)
    
    # Keyword density: ratio of content words to total words
    content_ratio = len(set(tokens)) / max(len(tokens), 1)
    
    return tfidf * pos_score * (0.5 + content_ratio)


def extractive_summary(text: str, ratio: float = 0.3) -> str:
    """Generate extractive summary by selecting top-scoring sentences."""
    sentences = split_sentences(text)
    if not sentences:
        return ""
    
    n_sentences = max(1, int(len(sentences) * ratio))
    tf_list = compute_tf(sentences)
    idf = compute_idf(sentences)
    
    scored = [(score_sentence(s, tf_list, idf, i, len(sentences)), i, s)
              for i, s in enumerate(sentences)]
    scored.sort(reverse=True)
    
    # Reorder by original position for coherence
    top_sentences = sorted(scored[:n_sentences], key=lambda x: x[1])
    return " ".join(s for _, _, s in top_sentences)


def main():
    parser = argparse.ArgumentParser(
        description="Generate an extractive summary of a text document."
    )
    parser.add_argument("input", nargs="?", type=str, default=None,
                        help="Input text file (stdin if omitted)")
    parser.add_argument("-r", "--ratio", type=float, default=0.3,
                        help="Summary length ratio (0–1, default: 0.3)")
    args = parser.parse_args()
    
    if args.input:
        with open(args.input, "r", encoding="utf-8") as f:
            text = f.read()
    else:
        text = sys.stdin.read()
    
    summary = extractive_summary(text, args.ratio)
    print(summary)


if __name__ == "__main__":
    main()
