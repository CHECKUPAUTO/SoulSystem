#!/usr/bin/env python3
"""
Adaptive Gradient Descent Tool

This tool implements an adaptive gradient descent optimizer using a backtracking
line search with the Armijo condition to dynamically adjust the learning rate.
It is inspired by modern adaptive optimization techniques described in recent
research on self-tuning learning rates.

Usage:
    python adaptive_gradient_descent.py --func quadratic --x0 2.0 1.0 --max_iter 100
"""

import argparse
import math
import sys
from typing import Callable, Tuple


def armijo_line_search(f: Callable, grad_f: Callable, x: list, direction: list,
                       alpha: float = 1.0, rho: float = 0.5, c: float = 1e-4,
                       max_iter: int = 20) -> float:
    """
    Backtracking line search satisfying the Armijo condition.
    Returns the step size (learning rate) for the current iteration.
    """
    fx = f(x)
    grad_x = grad_f(x)
    grad_dot_dir = sum(g * d for g, d in zip(grad_x, direction))
    
    for _ in range(max_iter):
        x_new = [xi + alpha * di for xi, di in zip(x, direction)]
        if f(x_new) <= fx + c * alpha * grad_dot_dir:
            return alpha
        alpha *= rho
    return alpha


def adaptive_gradient_descent(f: Callable, grad_f: Callable, x0: list,
                              max_iter: int = 100, tol: float = 1e-6) -> Tuple[list, list]:
    """
    Perform adaptive gradient descent with Armijo line search.
    
    Args:
        f: Objective function
        grad_f: Gradient of f
        x0: Initial point
        max_iter: Maximum number of iterations
        tol: Convergence tolerance on gradient norm
    
    Returns:
        (x_opt, path): Optimal point and trajectory of points
    """
    x = list(x0)
    path = [x.copy()]
    
    for _ in range(max_iter):
        grad = grad_f(x)
        grad_norm = math.sqrt(sum(g * g for g in grad))
        if grad_norm < tol:
            break
        
        direction = [-g for g in grad]  # steepest descent direction
        alpha = armijo_line_search(f, grad_f, x, direction)
        x = [xi + alpha * di for xi, di in zip(x, direction)]
        path.append(x.copy())
    
    return x, path


def main():
    parser = argparse.ArgumentParser(
        description="Adaptive gradient descent with Armijo line search"
    )
    parser.add_argument("--func", type=str, default="quadratic",
                        choices=["quadratic", "rosenbrock"],
                        help="Objective function to minimize")
    parser.add_argument("--x0", type=float, nargs="+", default=[2.0, 2.0],
                        help="Initial point (e.g., --x0 2.0 2.0)")
    parser.add_argument("--max_iter", type=int, default=100,
                        help="Maximum number of iterations")
    
    args = parser.parse_args()
    
    # Define functions
    if args.func == "quadratic":
        def f(x): return x[0]**2 + x[1]**2
        def grad_f(x): return [2*x[0], 2*x[1]]
    else:  # rosenbrock
        def f(x): return (1 - x[0])**2 + 100*(x[1] - x[0]**2)**2
        def grad_f(x): 
            dx0 = -2*(1 - x[0]) - 400*x[0]*(x[1] - x[0]**2)
            dx1 = 200*(x[1] - x[0]**2)
            return [dx0, dx1]
    
    x_opt, path = adaptive_gradient_descent(f, grad_f, args.x0, args.max_iter)
    
    print(f"Function: {args.func}")
    print(f"Initial point: {args.x0}")
    print(f"Optimal point: [{x_opt[0]:.6f}, {x_opt[1]:.6f}]")
    print(f"Final value: {f(x_opt):.10f}")
    print(f"Iterations: {len(path)}")


if __name__ == "__main__":
    main()
