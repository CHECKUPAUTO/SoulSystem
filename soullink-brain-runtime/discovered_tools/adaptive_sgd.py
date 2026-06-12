#!/usr/bin/env python3
"""
Adaptive SGD: Optimisation adaptative avec correction de biais temporel.

Basé sur des idées d'adaptation dynamique de taux d'apprentissage
pour améliorer la convergence dans des environnements non-stationnaires.
"""

import argparse
import numpy as np


def loss_and_grad(x: np.ndarray) -> tuple[float, np.ndarray]:
    """Fonction de perte quadratique simple: f(x) = ||x||^2 / 2."""
    return 0.5 * np.dot(x, x), x


def adaptive_sgd(
    dim: int = 10,
    steps: int = 1000,
    lr_init: float = 0.1,
    beta: float = 0.9,
    eps: float = 1e-8,
    seed: int | None = None,
) -> tuple[np.ndarray, list[float]]:
    """
    Exécute SGD avec taux d'apprentissage adaptatif et correction de biais.

    Utilise une moyenne exponentielle du gradient pour ajuster le pas.
    Similaire à Adam mais simplifié pour démonstration.

    Args:
        dim: Dimension de l'espace de paramètres.
        steps: Nombre d'itérations.
        lr_init: Taux d'apprentissage initial.
        beta: Facteur d'atténuation pour la moyenne mobile.
        eps: Constante de stabilité.
        seed: Graine aléatoire pour reproductibilité.

    Returns:
        tuple: (solution finale, liste des pertes)
    """
    rng = np.random.default_rng(seed)
    x = rng.normal(scale=1.0, size=dim)
    m = np.zeros_like(x)  # Moyenne mobile du gradient
    losses = []

    for t in range(1, steps + 1):
        # Gradient bruité (simulation de données non-stationnaires)
        noise = rng.normal(scale=0.1, size=dim)
        _, grad = loss_and_grad(x + noise)

        # Mise à jour de la moyenne mobile
        m = beta * m + (1 - beta) * grad

        # Correction de biais temporel
        m_hat = m / (1 - beta**t)

        # Taux d'apprentissage adaptatif (inverse proportionnel à la norme du gradient estimé)
        lr = lr_init / (1 + 0.01 * np.linalg.norm(m_hat))

        # Mise à jour du paramètre
        x = x - lr * m_hat

        # Enregistrer la perte réelle (sans bruit)
        loss, _ = loss_and_grad(x)
        losses.append(loss)

    return x, losses


def main():
    parser = argparse.ArgumentParser(
        description="Optimisation adaptative SGD avec correction de biais temporel."
    )
    parser.add_argument("--steps", type=int, default=500, help="Nombre d'itérations")
    parser.add_argument("--dim", type=int, default=5, help="Dimension du problème")
    parser.add_argument("--lr_init", type=float, default=0.1, help="Taux d'apprentissage initial")
    parser.add_argument("--beta", type=float, default=0.9, help="Facteur d'atténuation")
    parser.add_argument("--seed", type=int, default=42, help="Graine aléatoire")
    args = parser.parse_args()

    x_final, losses = adaptive_sgd(
        dim=args.dim,
        steps=args.steps,
        lr_init=args.lr_init,
        beta=args.beta,
        seed=args.seed,
    )

    print(f"Solution finale: {x_final}")
    print(f"Perte finale: {losses[-1]:.6f}")
    print(f"Réduction de la perte: {losses[0]:.6f} → {losses[-1]:.6f}")


if __name__ == "__main__":
    main()
