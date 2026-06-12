#!/usr/bin/env python3
"""Adaptive Shielding for Reinforcement Learning Agents.

This tool implements a simple adaptive shielding mechanism that protects
an RL agent by constraining its actions when the environment is uncertain
or potentially dangerous, inspired by recent shielding techniques in RL.
"""

import argparse
import numpy as np
from typing import List, Tuple


def compute_safety_margin(state: np.ndarray, safe_threshold: float = 0.5) -> float:
    """Compute safety margin based on state norm.
    
    Args:
        state: Current state vector.
        safe_threshold: Threshold below which state is considered safe.
    
    Returns:
        Safety margin: positive if safe, negative if unsafe.
    """
    return safe_threshold - np.linalg.norm(state)


def shielded_action(action: np.ndarray, state: np.ndarray, 
                    safe_threshold: float = 0.5, alpha: float = 0.3) -> np.ndarray:
    """Apply adaptive shielding: blend safe action with original action.
    
    Args:
        action: Original agent action.
        state: Current state.
        safe_threshold: Safety threshold for state norm.
        alpha: Shielding strength (0=none, 1=full override).
    
    Returns:
        Shielded action.
    """
    margin = compute_safety_margin(state, safe_threshold)
    if margin >= 0:
        return action  # Safe: no shielding
    
    # Compute safe action: clamp toward zero (stop/neutral)
    safe_action = np.clip(action, -np.abs(margin), np.abs(margin))
    return (1 - alpha) * action + alpha * safe_action


def simulate_agent(steps: int = 20, seed: int = 42) -> List[Tuple[np.ndarray, np.ndarray, np.ndarray]]:
    """Simulate agent with adaptive shielding.
    
    Args:
        steps: Number of simulation steps.
        seed: Random seed for reproducibility.
    
    Returns:
        List of (state, raw_action, shielded_action) tuples.
    """
    np.random.seed(seed)
    state = np.random.randn(3) * 0.8  # Start near boundary
    results = []
    
    for _ in range(steps):
        raw_action = np.random.randn(3) * 0.6
        shielded = shielded_action(raw_action, state)
        state = state + 0.1 * shielded + 0.05 * np.random.randn(3)
        results.append((state.copy(), raw_action, shielded))
    
    return results


def main():
    parser = argparse.ArgumentParser(description="Adaptive Shielding Demo for RL Agents")
    parser.add_argument("--steps", type=int, default=20, help="Simulation steps")
    parser.add_argument("--seed", type=int, default=42, help="Random seed")
    parser.add_argument("--threshold", type=float, default=0.5, help="Safety threshold")
    args = parser.parse_args()
    
    results = simulate_agent(steps=args.steps, seed=args.seed)
    print("Step | State Norm | Raw Action Norm | Shielded Action Norm")
    print("-" * 60)
    for i, (state, raw, shielded) in enumerate(results):
        print(f"{i:4d} | {np.linalg.norm(state):10.3f} | {np.linalg.norm(raw):15.3f} | {np.linalg.norm(shielded):20.3f}")


if __name__ == "__main__":
    main()