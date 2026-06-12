#!/usr/bin/env python3
"""
Simulateur d'optimisation stochastique inspiré de arXiv:2605.00777v1.
Implémente une adaptation dynamique du pas de learning basée sur la variance estimée des gradients.
"""

import argparse
import math
import random
from typing import Callable, List, Tuple


def estimate_variance(gradients: List[float], eps: float = 1e-12) -> float:
    """Estime la variance d'une liste de gradients."""
    if len(gradients) < 2:
        return 1.0
    mean = sum(gradients) / len(gradients)
    var = sum((g - mean) ** 2 for g in gradients) / (len(gradients) - 1)
    return max(var, eps)


def simulate_optimization(
    f: Callable[[float], float],
    f_prime: Callable[[float], float],
    x0: float,
    steps: int,
    base_lr: float,
    window: int = 5,
    decay: float = 0.99
) -> Tuple[List[float], List[float]]:
    """
    Simule l'optimisation avec adaptation dynamique du pas de learning.
    Retourne les positions et les pas de learning utilisés.
    """
    x = x0
    positions = [x]
    lrs = [base_lr]
    grad_window: List[float] = []

    for _ in range(steps):
        # Gradient stochastique (ajout de bruit gaussien pour simuler la variance)
        noise = random.gauss(0, 0.1)
        grad = f_prime(x) + noise
        grad_window.append(grad)
        if len(grad_window) > window:
            grad_window.pop(0)

        # Estimation de la variance et adaptation du pas
        var = estimate_variance(grad_window)
        adaptive_lr = base_lr / (1 + var * base_lr)
        adaptive_lr = max(adaptive_lr, base_lr * 1e-4)  # plafond inférieur
        adaptive_lr *= decay  # décroissance globale

        # Mise à jour
        x = x - adaptive_lr * grad
        positions.append(x)
        lrs.append(adaptive_lr)

    return positions, lrs


def main():
    parser = argparse.ArgumentParser(
        description="Simulateur d'optimisation stochastique adaptative (arXiv:2605.00777v1)"
    )
    parser.add_argument("--steps", type=int, default=100, help="Nombre d'itérations")
    parser.add_argument("--x0", type=float, default=5.0, help="Point de départ")
    parser.add_argument("--base_lr", type=float, default=0.1, help="Pas de learning de base")
    parser.add_argument("--window", type=int, default=5, help="Taille de la fenêtre pour la variance")
    parser.add_argument("--decay", type=float, default=0.99, help="Facteur de décroissance globale")
    args = parser.parse_args()

    # Fonction de test: f(x) = (x-2)^2
    def f(x: float) -> float:
        return (x - 2.0) ** 2

    def f_prime(x: float) -> float:
        return 2.0 * (x - 2.0)

    positions, lrs = simulate_optimization(
        f=f,
        f_prime=f_prime,
        x0=args.x0,
        steps=args.steps,
        base_lr=args.base_lr,
        window=args.window,
        decay=args.decay
    )

    print(f"Convergence: x_final = {positions[-1]:.6f}, target = 2.0")
    print(f"Dernier pas de learning: {lrs[-1]:.6f}")


if __name__ == "__main__":
    main()
