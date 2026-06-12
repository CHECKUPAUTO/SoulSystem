#!/usr/bin/env python3
"""
Implementation of a discrete-time diffusion model's conditional probability visualization.
Based on the methodology described in arXiv:2605.00264v1.

This script simulates and visualizes the evolution of the probability distribution
over states in a simple diffusion process, demonstrating key concepts from the paper.
"""

import argparse
import numpy as np
import matplotlib.pyplot as plt


def compute_transition_matrix(n_states: int, diffusion_rate: float) -> np.ndarray:
    """Build a symmetric transition matrix for a discrete diffusion process."""
    P = np.zeros((n_states, n_states))
    for i in range(n_states):
        if i == 0:
            P[i, i] = 1 - diffusion_rate
            P[i, i + 1] = diffusion_rate
        elif i == n_states - 1:
            P[i, i - 1] = diffusion_rate
            P[i, i] = 1 - diffusion_rate
        else:
            P[i, i - 1] = diffusion_rate / 2
            P[i, i] = 1 - diffusion_rate
            P[i, i + 1] = diffusion_rate / 2
    return P


def evolve_distribution(P: np.ndarray, p0: np.ndarray, n_steps: int) -> np.ndarray:
    """Evolve initial distribution p0 through n_steps steps of the Markov chain."""
    p = p0.copy()
    history = [p.copy()]
    for _ in range(n_steps):
        p = P @ p
        history.append(p.copy())
    return np.array(history)


def plot_evolution(history: np.ndarray, title: str = "Probability Distribution Evolution"):
    """Plot the evolution of the probability distribution over time."""
    n_steps, n_states = history.shape
    fig, ax = plt.subplots(figsize=(8, 5))
    im = ax.imshow(history.T, aspect='auto', origin='lower', cmap='viridis',
                   extent=[0, n_steps - 1, 0, n_states - 1])
    ax.set_xlabel('Time step')
    ax.set_ylabel('State')
    ax.set_title(title)
    fig.colorbar(im, label='Probability')
    plt.tight_layout()
    plt.show()


def main():
    parser = argparse.ArgumentParser(
        description="Visualize diffusion process probability evolution (arXiv:2605.00264v1)"
    )
    parser.add_argument('--n_states', type=int, default=10,
                        help='Number of discrete states (default: 10)')
    parser.add_argument('--diffusion_rate', type=float, default=0.3,
                        help='Diffusion rate per step (0 <= r <= 1, default: 0.3)')
    parser.add_argument('--n_steps', type=int, default=20,
                        help='Number of evolution steps (default: 20)')
    parser.add_argument('--initial_state', type=int, default=0,
                        help='Index of initial state (default: 0)')
    args = parser.parse_args()

    # Input validation
    if not (0 <= args.diffusion_rate <= 1):
        raise ValueError("diffusion_rate must be in [0, 1]")
    if not (0 <= args.initial_state < args.n_states):
        raise ValueError("initial_state must be in [0, n_states-1]")

    # Build transition matrix and initial distribution
    P = compute_transition_matrix(args.n_states, args.diffusion_rate)
    p0 = np.zeros(args.n_states)
    p0[args.initial_state] = 1.0

    # Evolve and visualize
    history = evolve_distribution(P, p0, args.n_steps)
    plot_evolution(history, title="Diffusion Process: Probability Distribution Evolution")


if __name__ == "__main__":
    main()
