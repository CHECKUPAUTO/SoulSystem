#!/usr/bin/env python3
"""
Abstraction Cost Analyzer

Démontre l'idée centrale du paper "The Hidden Costs of Great Abstractions":
les abstractions (comme les compréhensions, les fonctions, les bibliothèques)
peuvent cacher des coûts en termes de performance, lisibilité, ou complexité cognitive.

Ce script compare deux implémentations équivalentes d'un traitement de données:
- une version impérative (boucle explicite)
- une version abstraite (compréhension + fonctions intégrées)

Il mesure le temps d'exécution et évalue la complexité cognitive via des heuristiques.
"""

import argparse
import time
import statistics
from typing import List, Callable, Dict, Any


def measure_execution_time(func: Callable, data: List[int], iterations: int = 100) -> float:
    """Mesure le temps moyen d'exécution d'une fonction sur plusieurs itérations."""
    times = []
    for _ in range(iterations):
        start = time.perf_counter()
        _ = func(data)
        times.append(time.perf_counter() - start)
    return statistics.mean(times)


def cognitive_complexity_estimate(code_snippet: str) -> int:
    """
    Estime la complexité cognitive d'un snippet de code via des heuristiques simples:
    - Compte les mots-clés de contrôle (if, for, while, etc.)
    - Pénalise les niveaux d'imbrication
    - Note: c'est une approximation grossière, mais utile pour la démonstration
    """
    keywords = ['if', 'for', 'while', 'with', 'try', 'except', 'finally', 'elif', 'else']
    complexity = 0
    lines = code_snippet.splitlines()
    indent_level = 0
    for line in lines:
        stripped = line.strip()
        if not stripped or stripped.startswith('#'):
            continue
        # Compter les mots-clés
        for kw in keywords:
            if kw in stripped and not stripped.startswith('#'):
                complexity += 1
        # Estimer l'imbrication via les espaces initiaux
        indent_level = len(line) - len(line.lstrip())
        complexity += indent_level // 4  # ~1 unité par niveau de 4 espaces
    return complexity


def imperative_sum(data: List[int]) -> int:
    """Version impérative: boucle explicite."""
    total = 0
    for x in data:
        total += x
    return total


def abstracted_sum(data: List[int]) -> int:
    """Version abstraite: utilisation de sum() et compréhension."""
    return sum(x for x in data)


def main():
    parser = argparse.ArgumentParser(
        description="Analyse les coûts cachés des abstractions en Python."
    )
    parser.add_argument(
        '--size', type=int, default=10_000,
        help="Taille du jeu de données (défaut: 10 000)"
    )
    parser.add_argument(
        '--iterations', type=int, default=100,
        help="Nombre d'itérations pour la moyenne (défaut: 100)"
    )
    args = parser.parse_args()

    # Générer des données
    data = list(range(args.size))

    # Mesurer les temps
    t_imperative = measure_execution_time(imperative_sum, data, args.iterations)
    t_abstracted = measure_execution_time(abstracted_sum, data, args.iterations)

    # Estimer la complexité cognitive
    c_imperative = cognitive_complexity_estimate(inspect.getsource(imperative_sum))
    c_abstracted = cognitive_complexity_estimate(inspect.getsource(abstracted_sum))

    # Résultats
    print("=== Abstraction Cost Analysis ===")
    print(f"Taille des données: {args.size:,}")
    print(f"Itérations: {args.iterations}")
    print(f"\nTemps d'exécution (moyen, en secondes):")
    print(f"  Impérative: {t_imperative:.6f}")
    print(f"  Abstraite : {t_abstracted:.6f}")
    print(f"  Ratio (Imp/Abstr): {t_imperative / t_abstracted:.2f}x")
    print(f"\nComplexité cognitive estimée:")
    print(f"  Impérative: {c_imperative}")
    print(f"  Abstraite : {c_abstracted}")
    print("\nConclusion: Les abstractions peuvent réduire la complexité cognitive,")
    print("mais peuvent cacher des coûts de performance (ici, sum() + générateur est souvent plus lent).")


if __name__ == "__main__":
    import inspect  # Importé ici pour éviter un import global inutile
    main()
