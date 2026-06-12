#!/usr/bin/env python3
"""
Adaptive-step SDE integrator inspired by arXiv:2602.07200v2.
Implements a local stability-aware adaptive time-stepping scheme for SDEs.
"""

import argparse
import numpy as np
from typing import Callable, Tuple


def drift(x: np.ndarray, t: float) -> np.ndarray:
    """Linear drift: -x"""
    return -x


def diffusion(x: np.ndarray, t: float) -> np.ndarray:
    """Constant diffusion: 0.5"""
    return 0.5 * np.ones_like(x)


def euler_maruyama_step(x: np.ndarray, t: float, dt: float, dW: np.ndarray,
                        drift_fn: Callable, diff_fn: Callable) -> np.ndarray:
    """Standard Euler–Maruyama step."""
    return x + drift_fn(x, t) * dt + diff_fn(x, t) * dW


def compute_local_error_estimate(x_new: np.ndarray, x_half: np.ndarray,
                                 drift_fn: Callable, diff_fn: Callable,
                                 t: float, dt: float) -> float:
    """
    Local error estimate via embedded method: compare full step vs two half steps.
    Uses strong order 0.5 error proxy.
    """
    dW = np.random.normal(0, np.sqrt(dt), size=x_new.shape)
    x_half_new = euler_maruyama_step(x_half, t + dt/2, dt/2, dW/2, drift_fn, diff_fn)
    return np.linalg.norm(x_new - x_half_new) / (1 + np.linalg.norm(x_new))


def integrate_sde_adaptive(drift_fn: Callable, diff_fn: Callable,
                           x0: np.ndarray, t0: float, tf: float,
                           dt_min: float = 1e-4, dt_max: float = 0.1,
                           atol: float = 1e-3, rtol: float = 1e-2,
                           seed: int = 42) -> Tuple[np.ndarray, np.ndarray]:
    """
    Integrate SDE with adaptive time stepping based on local stability.
    Returns (times, trajectory).
    """
    rng = np.random.default_rng(seed)
    t = t0
    x = np.array(x0, dtype=float)
    times = [t]
    traj = [x.copy()]
    dt = dt_max

    while t < tf - 1e-12:
        if t + dt > tf:
            dt = tf - t
        dW = rng.normal(0, np.sqrt(dt), size=x.shape)
        x_new = euler_maruyama_step(x, t, dt, dW, drift_fn, diff_fn)
        x_half = euler_maruyama_step(x, t, dt/2, dW/2, drift_fn, diff_fn)
        err = compute_local_error_estimate(x_new, x_half, drift_fn, diff_fn, t, dt)

        # Stability check: reject if drift amplifies too much
        if np.any(np.abs(x_new) > 10 * np.abs(x)):
            err = np.inf

        if err <= atol + rtol * np.linalg.norm(x_new):
            t += dt
            x = x_new
            times.append(t)
            traj.append(x.copy())
            # Increase step if error is small
            dt = min(dt_max, 1.25 * dt * (atol + rtol * np.linalg.norm(x)) / max(err, 1e-15))
        else:
            dt = max(dt_min, 0.8 * dt * (atol + rtol * np.linalg.norm(x)) / max(err, 1e-15))

    return np.array(times), np.array(traj)


def main():
    parser = argparse.ArgumentParser(
        description="Adaptive-step SDE integrator (arXiv:2602.07200v2-inspired)"
    )
    parser.add_argument("--t0", type=float, default=0.0, help="Initial time")
    parser.add_argument("--tf", type=float, default=2.0, help="Final time")
    parser.add_argument("--x0", type=float, nargs="+", default=[1.0],
                        help="Initial condition (space-separated)")
    parser.add_argument("--seed", type=int, default=42, help="Random seed")
    parser.add_argument("--output", type=str, default="trajectory.npy",
                        help="Output file for trajectory")
    args = parser.parse_args()

    times, traj = integrate_sde_adaptive(
        drift, diffusion, args.x0, args.t0, args.tf, seed=args.seed
    )
    np.save(args.output, np.column_stack((times, traj)))
    print(f"Saved trajectory to {args.output}")


if __name__ == "__main__":
    main()
