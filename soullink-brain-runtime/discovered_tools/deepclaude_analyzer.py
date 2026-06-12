#!/usr/bin/env python3
"""DeepClaude Analyzer - Analyse sémantique de profondeur basée sur embeddings moyens.

Cet outil implémente une version simplifiée de l'approche DeepClaude pour mesurer
la profondeur sémantique des phrases en comparant leurs embeddings moyens.
"""

import argparse
import sys
from typing import List, Tuple
import numpy as np

# Simule un vocabulaire et des embeddings pré-entraînés (ex. GloVe-like)
# Dans un usage réel, on utiliserait des embeddings pré-entraînés (ex. via gensim)
EMBEDDING_DIM = 5
VOCAB = {
    "the": [0.1, 0.2, 0.3, 0.4, 0.5],
    "cat": [0.9, 0.1, 0.2, 0.8, 0.3],
    "sat": [0.2, 0.7, 0.6, 0.1, 0.9],
    "on": [0.3, 0.4, 0.5, 0.2, 0.6],
    "mat": [0.8, 0.2, 0.9, 0.3, 0.1],
    "dog": [0.7, 0.3, 0.1, 0.9, 0.4],
    "ran": [0.4, 0.6, 0.8, 0.2, 0.7],
    "quickly": [0.5, 0.5, 0.4, 0.7, 0.2],
    "slowly": [0.2, 0.3, 0.7, 0.4, 0.8],
}

def tokenize(text: str) -> List[str]:
    """Tokenise une phrase en mots minuscules."""
    return text.lower().strip().split()

def get_embedding(word: str) -> np.ndarray:
    """Renvoie l'embedding d'un mot (zéro si absent)."""
    return np.array(VOCAB.get(word, [0.0] * EMBEDDING_DIM))

def sentence_embedding(sentence: str) -> np.ndarray:
    """Calcule l'embedding moyen d'une phrase."""
    tokens = tokenize(sentence)
    if not tokens:
        return np.zeros(EMBEDDING_DIM)
    embeddings = [get_embedding(t) for t in tokens]
    return np.mean(embeddings, axis=0)

def cosine_similarity(a: np.ndarray, b: np.ndarray) -> float:
    """Calcule la similarité cosinus entre deux vecteurs."""
    norm_a = np.linalg.norm(a)
    norm_b = np.linalg.norm(b)
    if norm_a == 0 or norm_b == 0:
        return 0.0
    return float(np.dot(a, b) / (norm_a * norm_b))

def analyze_depth(sentences: List[str]) -> List[Tuple[str, float]]:
    """Analyse la profondeur sémantique relative des phrases."""
    if not sentences:
        return []
    embeddings = [sentence_embedding(s) for s in sentences]
    # Profondeur = distance moyenne à toutes les autres (inversion pour similitude)
    depths = []
    for i, emb in enumerate(embeddings):
        similarities = [cosine_similarity(emb, e) for j, e in enumerate(embeddings) if i != j]
        avg_sim = np.mean(similarities) if similarities else 0.0
        # Profondeur = 1 - similarité moyenne (plus proche = moins profond)
        depths.append(1.0 - avg_sim)
    return list(zip(sentences, depths))

def main():
    parser = argparse.ArgumentParser(
        description="Analyse sémantique de profondeur de phrases (inspiré de DeepClaude)."
    )
    parser.add_argument(
        "--sentences", "-s",
        nargs="+",
        default=["the cat sat on the mat", "the dog ran quickly"],
        help="Liste de phrases à analyser (défaut: 'the cat sat on the mat' et 'the dog ran quickly')"
    )
    args = parser.parse_args()
    
    results = analyze_depth(args.sentences)
    print("\nRésultats d'analyse de profondeur sémantique:")
    for sentence, depth in results:
        print(f"  Profondeur: {depth:.3f} → '{sentence}'")

if __name__ == "__main__":
    main()