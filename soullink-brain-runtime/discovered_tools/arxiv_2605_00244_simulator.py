#!/usr/bin/env python3
"""
Simulateur de décomposition spectrale rapide pour matrices creuses.
Implémente une approximation de la décomposition en valeurs propres
basée sur la méthode de projection stochastique ( proposition 3.2 ).

Reference: arXiv:2605.00244v1 [cs.LG] 1 May 2026
"""

import argparse
import numpy as np
from numpy.linalg import eigh


def generate_sparse_matrix(n: int, density: float = 0.05, seed: int | None = None) -> np.ndarray:
    """Génère une matrice symétrique creuse de taille n×n."""
    rng = np.random.default_rng(seed)
    A = rng.normal(0, 1, (n, n)) * (rng.random((n, n)) < density)
    A = (A + A.T) / 2  # symétrisation
    np.fill_diagonal(A, 0)
    return A


def fast_eig_approx(A: np.ndarray, k: int, n_iter: int = 10, seed: int | None = None) -> tuple[np.ndarray, np.ndarray]:
    """
    Approximation rapide des k plus grandes valeurs propres et vecteurs propres.
    Utilise une projection stochastique (algorithme 1 simplifié).
    """
    rng = np.random.default_rng(seed)
    n = A.shape[0]
    # Initialisation aléatoire
    Q = rng.standard_normal((n, k))
    Q, _ = np.linalg.qr(Q)
    
    # Itérations de projection
    for _ in range(n_iter):
        Z = A @ Q
        Q, _ = np.linalg.qr(Z)
    
    # Récupération des valeurs propres via Rayleigh-Ritz
    B = Q.T @ A @ Q
    w, v = eigh(B)
    idx = np.argsort(w)[::-1][:k]
    return w[idx], Q @ v[:, idx]


def main():
    parser = argparse.ArgumentParser(
        description="Simulateur de décomposition spectrale rapide pour matrices creuses (arXiv:2605.00244v1)."
    )
    parser.add_argument("-n", "--size", type=int, default=100, help="Taille de la matrice (défaut: 100)")
    parser.add_argument("-k", "--rank", type=int, default=5, help="Nombre de composantes à extraire (défaut: 5)")
    parser.add_argument("-d", "--density", type=float, default=0.05, help="Densité de la matrice (0-1, défaut: 0.05)")
    parser.add_argument("-s", "--seed", type=int, default=42, help="Graine aléatoire (défaut: 42)")
    parser.add_argument("-i", "--iter", type=int, default=10, help="Nombre d'itérations (défaut: 10)")
    args = parser.parse_args()
    
    A = generate_sparse_matrix(args.size, args.density, args.seed)
    w_approx, v_approx = fast_eig_approx(A, args.rank, args.iter, args.seed)
    w_exact, _ = eigh(A)
    w_exact = np.sort(w_exact)[::-1][:args.rank]
    
    error = np.linalg.norm(w_approx - w_exact) / np.linalg.norm(w_exact)
    print(f"Matrice {args.size}×{args.size}, densité={args.density:.2%}")
    print(f"Valeurs propres approximatives (top {args.rank}): {w_approx}")
    print(f"Valeurs propres exactes (top {args.rank}): {w_exact}")
    print(f"Erreur relative: {error:.2e}")


if __name__ == "__main__":
    main()