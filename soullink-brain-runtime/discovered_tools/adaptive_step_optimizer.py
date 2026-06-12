#!/usr/bin/env python3
"""
Adaptive Step Optimizer - Implementation of an adaptive step-size optimization method
inspired by recent adaptive optimization techniques (e.g., Armijo line search with
backtracking and convergence guarantees).

Reference: arXiv:2605.00260v1 (hypothetical adaptive optimization framework).
"""

import argparse
import math
import sys
from typing import Callable, Tuple

def armijo_backtracking(
    f: Callable[[float], float],
    x: float,
    direction: float,
    grad: float,
    step: float = 1.0,
    rho: float = 0.5,
    c: float = 1e-4,
    max_iter: int = 20
) -> float:
    """
    Backtracking line search satisfying the Armijo condition.
    
    Args:
        f: Objective function
        x: Current point
        direction: Search direction (typically -grad for gradient descent)
        grad: Gradient at x
        step: Initial step size
        rho: Reduction factor (0 < rho < 1)
        c: Armijo parameter (0 < c < 1)
        max_iter: Maximum number of backtracking steps
    
    Returns:
        Optimized step size satisfying Armijo condition
    """
    fx = f(x)
    slope = grad * direction
    for _ in range(max_iter):
        if f(x + step * direction) <= fx + c * step * slope:
            return step
        step *= rho
    return step

def adaptive_optimizer(
    f: Callable[[float], float],
    x0: float,
    lr: float = 0.1,
    tol: float = 1e-6,
    max_iter: int = 1000
) -> Tuple[float, float, int]:
    """
    Adaptive step-size optimizer using gradient descent with Armijo backtracking.
    
    Args:
        f: Objective function (differentiable in practice)
        x0: Initial point
        lr: Initial learning rate (used as starting step)
        tol: Convergence tolerance on gradient norm
        max_iter: Maximum number of iterations
    
    Returns:
        Tuple of (optimal_x, f(optimal_x), iterations)
    """
    x = x0
    for i in range(1, max_iter + 1):
        # Numerical gradient (central difference for smoothness)
        h = 1e-8
        grad = (f(x + h) - f(x - h)) / (2 * h)
        
        if abs(grad) < tol:
            break
        
        direction = -grad
        step = armijo_backtracking(f, x, direction, grad, step=lr)
        x += step * direction
    
    return x, f(x), i

def main():
    parser = argparse.ArgumentParser(
        description="Adaptive step-size optimizer using Armijo backtracking line search."
    )
    parser.add_argument(
        "--func", type=str, default="(x-2)**2",
        help="Python expression for objective function f(x) (default: '(x-2)**2')"
    )
    parser.add_argument(
        "--x0", type=float, default=0.0,
        help="Initial point (default: 0.0)"
    )
    parser.add_argument(
        "--lr", type=float, default=1.0,
        help="Initial learning rate (default: 1.0)"
    )
    parser.add_argument(
        "--tol", type=float, default=1e-6,
        help="Convergence tolerance (default: 1e-6)"
    )
    parser.add_argument(
        "--max_iter", type=int, default=1000,
        help="Maximum iterations (default: 1000)"
    )
    
    args = parser.parse_args()
    
    # Compile function safely
    try:
        f = eval(f"lambda x: {args.func}")
    except Exception as e:
        sys.exit(f"Error parsing function: {e}")
    
    x_opt, f_opt, iters = adaptive_optimizer(
        f, args.x0, lr=args.lr, tol=args.tol, max_iter=args.max_iter
    )
    
    print(f"Minimum found at x = {x_opt:.6f}")
    print(f"f(x) = {f_opt:.6f}")
    print(f"Iterations: {iters}")

if __name__ == "__main__":
    main()