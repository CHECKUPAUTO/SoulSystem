#!/usr/bin/env python3
"""
Simulateur de système quantique à temps discret avec correction d'erreurs adaptative.
Basé sur l'article arXiv:2605.00654v1 (idée clé: détection et correction en temps réel via mesures non destructives).
"""

import argparse
import numpy as np
import random


def apply_X(state: np.ndarray) -> np.ndarray:
    """Applique la porte X (bit-flip) à l'état quantique."""
    return np.array([state[1], state[0], state[3], state[2]])


def apply_Z(state: np.ndarray) -> np.ndarray:
    """Applique la porte Z (phase-flip) à l'état quantique."""
    return np.array([state[0], state[1], -state[2], -state[3]])


def measure_syndrome(state: np.ndarray) -> tuple:
    """Simule une mesure de syndrome (Z1Z2 et X1X2) pour détecter les erreurs."""
    # Probabilité de détection basée sur les amplitudes
    p_z = abs(state[2])**2 + abs(state[3])**2  # Z1Z2 = -1
    p_x = abs(state[1])**2 + abs(state[3])**2  # X1X2 = -1
    return (np.random.rand() < p_z, np.random.rand() < p_x)


def correct_error(state: np.ndarray, syndrome: tuple) -> np.ndarray:
    """Corrige les erreurs détectées selon le syndrome."""
    z_err, x_err = syndrome
    if z_err:
        state = apply_Z(state)
    if x_err:
        state = apply_X(state)
    return state


def simulate_step(state: np.ndarray, error_rate: float) -> np.ndarray:
    """Effectue une étape de simulation: erreur aléatoire + correction adaptative."""
    # Appliquer une erreur aléatoire
    if random.random() < error_rate:
        state = apply_X(state)
    if random.random() < error_rate:
        state = apply_Z(state)
    # Mesurer et corriger
    syndrome = measure_syndrome(state)
    state = correct_error(state, syndrome)
    return state


def main():
    parser = argparse.ArgumentParser(description="Simulateur quantique avec correction d'erreurs adaptative")
    parser.add_argument('--steps', type=int, default=100, help="Nombre d'étapes de simulation")
    parser.add_argument('--error_rate', type=float, default=0.05, help="Taux d'erreur par porte")
    parser.add_argument('--initial_state', type=str, default="00", help="État initial (00, 01, 10, 11)")
    args = parser.parse_args()

    # Initialiser l'état |00⟩ = [1, 0, 0, 0]
    state_map = {"00": np.array([1.0, 0, 0, 0]),
                 "01": np.array([0, 1.0, 0, 0]),
                 "10": np.array([0, 0, 1.0, 0]),
                 "11": np.array([0, 0, 0, 1.0])}
    state = state_map[args.initial_state]

    fidelity_history = []
    for _ in range(args.steps):
        state = simulate_step(state, args.error_rate)
        # Calcul de la fidélité par rapport à |00⟩
        fidelity = abs(state[0])**2
        fidelity_history.append(fidelity)

    print(f"Fidélité moyenne après {args.steps} étapes: {np.mean(fidelity_history):.4f}")


if __name__ == "__main__":
    main()