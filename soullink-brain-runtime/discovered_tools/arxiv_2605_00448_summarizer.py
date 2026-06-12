#!/usr/bin/env python3
"""
ArXiv 2605.00448v1 - Simplified TF-IDF Summarizer

Cet outil implémente une méthode de résumage automatique inspirée d'une approche
légère basée sur la pondération inverse de la fréquence des termes, sans dépendance
à des modèles lourds. Il sélectionne les phrases les plus informatives selon un score
composé de TF (fréquence dans la phrase) et IDF (rareté globale).
"""

import argparse
import re
import sys
from collections import Counter
from typing import List, Tuple


def tokenize(text: str) -> List[str]:
    """Tokenise le texte en mots minuscules, en gardant les lettres et chiffres."""
    return re.findall(r"\b[\w']+\b", text.lower())


def compute_tf(words: List[str]) -> dict:
    """Calcule la fréquence des mots dans une phrase."""
    counter = Counter(words)
    total = len(words) or 1
    return {word: count / total for word, count in counter.items()}


def compute_idf(documents: List[List[str]]) -> dict:
    """Calcule la fréquence inverse de document pour chaque mot."""
    doc_count = len(documents) or 1
    df = Counter()
    for doc in documents:
        for word in set(doc):
            df[word] += 1
    return {word: math.log(doc_count / (freq + 1) + 1) for word, freq in df.items()}


def summarize(text: str, max_sentences: int = 3) -> str:
    """Génère un résumé en sélectionnant les max_sentences phrases avec le score TF-IDF le plus élevé."""
    import math
    
    # Diviser en phrases (simplifié)
    sentences = re.split(r'(?<=[.!?])\s+', text.strip())
    sentences = [s for s in sentences if s.strip()]
    if not sentences:
        return ""
    
    # Tokeniser chaque phrase
    tokenized_sentences = [tokenize(s) for s in sentences]
    
    # Calculer IDF global
    idf = compute_idf(tokenized_sentences)
    
    # Calculer le score TF-IDF pour chaque phrase
    scores = []
    for i, words in enumerate(tokenized_sentences):
        tf = compute_tf(words)
        score = sum(tf[word] * idf.get(word, 0) for word in words)
        # Normalisation par longueur
        score /= (len(words) or 1)
        scores.append((i, score, sentences[i]))
    
    # Trier et sélectionner les meilleures phrases
    scores.sort(key=lambda x: x[1], reverse=True)
    top_indices = sorted([s[0] for s in scores[:max_sentences]])
    
    return " ".join(sentences[i] for i in top_indices)


def main():
    parser = argparse.ArgumentParser(
        description="Résumateur automatique léger basé sur TF-IDF simplifié (inspiré d'ArXiv:2605.00448v1)"
    )
    parser.add_argument("--input", "-i", type=str, help="Chemin vers un fichier texte (ou stdin si omis)")
    parser.add_argument("--sentences", "-n", type=int, default=3, help="Nombre de phrases à conserver")
    args = parser.parse_args()
    
    if args.input:
        with open(args.input, "r", encoding="utf-8") as f:
            text = f.read()
    else:
        text = sys.stdin.read()
    
    summary = summarize(text, max_sentences=args.sentences)
    print(summary)


if __name__ == "__main__":
    main()
