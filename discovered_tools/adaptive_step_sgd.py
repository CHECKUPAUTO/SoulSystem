#!/usr/bin/env python3
"""
Adaptive Step Stochastic Gradient Descent (AS-SGD)
Basé sur l'algorithme 1 du paper arXiv:2603.02275v2.
Utilise un pas adaptatif basé sur la variance estimée du gradient.
"""

import argparse
import numpy as np


def f(x):
    """Fonction de perte quadratique simple: f(x) = (x - 1)^2"""
    return (x - 1) ** 2


def grad_f(x):
    """Gradient de f: f'(x) = 2(x - 1)"""
    return 2 * (x - 1)


def noisy_grad(x, noise_std=0.5):
    """Gradient bruité gaussien"""
    return grad_f(x) + np.random.normal(0, noise_std)


def as_sgd(x0, n_steps=1000, eta0=0.1, alpha=0.5, gamma=0.55):
    """
    AS-SGD: Adaptive Step Stochastic Gradient Descent.
    
    Parameters:
        x0: point de départ
        n_steps: nombre d'itérations
        eta0: pas initial
        alpha: exposant pour la mise à jour du pas (0 < alpha < 1)
        gamma: seuil pour la variance (gamma > 0)
    
    Returns:
        liste des points x_t et des pas eta_t
    """
    x = x0
    eta = eta0
    x_history = [x0]
    eta_history = [eta0]
    
    for t in range(1, n_steps + 1):
        # Échantillons de gradient
        g1 = noisy_grad(x)
        g2 = noisy_grad(x)
        
        # Estimation de la variance
        var_est = ((g1 - g2) ** 2) / 2
        
        # Adaptation du pas
        if var_est > gamma * (eta ** (2 * alpha)):
            eta = eta * (t ** (-0.5))  # réduction du pas
        else:
            eta = eta * (t ** (-gamma))  # pas plus agressif
        
        # Mise à jour SGD
        g = noisy_grad(x)
        x = x - eta * g
        
        x_history.append(x)
        eta_history.append(eta)
    
    return x_history, eta_history


def main():
    parser = argparse.ArgumentParser(description="AS-SGD optimizer demo")
    parser.add_argument("--x0", type=float, default=5.0, help="Point de départ")
    parser.add_argument("--steps", type=int, default=500, help="Nombre d'itérations")
    parser.add_argument("--eta0", type=float, default=0.3, help="Pas initial")
    args = parser.parse_args()
    
    x_hist, eta_hist = as_sgd(args.x0, args.steps, args.eta0)
    
    print(f"\nFinal estimate: x = {x_hist[-1]:.6f} (target = 1)")
    print(f"Final step size: eta = {eta_hist[-1]:.6e}")
    print(f"Convergence error: {abs(x_hist[-1] - 1):.6e}")


if __name__ == "__main__":
    main()
