#!/usr/bin/env python3
"""
Adaptive Step ODE Solver using RKF45 (Runge-Kutta-Fehlberg 4(5)) method.
Implements adaptive step-size control for initial value problems.
Based on arXiv:2605.00689v1.
"""

import argparse
import math
import sys
from typing import Callable, List, Tuple


def rkf45_step(f: Callable[[float, List[float]], List[float]], t: float, y: List[float], h: float) -> Tuple[List[float], List[float]]:
    """Perform one RKF45 step, returning both 4th and 5th order solutions."""
    k1 = f(t, y)
    k2 = f(t + h/4, [y[i] + h*k1[i]/4 for i in range(len(y))])
    k3 = f(t + h*0.375, [y[i] + h*(3*k1[i] + 9*k2[i])/32 for i in range(len(y))])
    k4 = f(t + h*12/13, [y[i] + h*(1932*k1[i] - 7200*k2[i] + 7296*k3[i])/2197 for i in range(len(y))])
    k5 = f(t + h, [y[i] + h*(439*k1[i] - 810*k2[i] + 1236*k3[i] - 84*k4[i])/216 for i in range(len(y))])
    k6 = f(t + h/2, [y[i] + h*(-8*k1[i] + 72*k2[i] - 69*k3[i] + 68*k4[i] + 12*k5[i])/216 for i in range(len(y))])

    y4 = [y[i] + h*(25*k1[i] + 1408*k3[i] + 2197*k4[i] - k5[i])/2880 for i in range(len(y))]
    y5 = [y[i] + h*(16*k1[i] + 6656*k3[i] + 6561*k4[i] - 845*k5[i] + 2944*k6[i])/4275 for i in range(len(y))]
    return y4, y5


def solve_ivp(f: Callable[[float, List[float]], List[float]], t_span: Tuple[float, float], y0: List[float],
              rtol: float = 1e-6, atol: float = 1e-9, h0: float = 0.01) -> List[Tuple[float, List[float]]]:
    """Solve IVP using adaptive RKF45 with step control."""
    t, y = t_span[0], list(y0)
    h = min(h0, (t_span[1] - t) / 2)
    result = [(t, list(y))]

    while t < t_span[1] - 1e-14:
        if t + h > t_span[1]:
            h = t_span[1] - t
        y4, y5 = rkf45_step(f, t, y, h)
        err = max(abs(y5[i] - y4[i]) / (atol + rtol * max(abs(y4[i]), abs(y5[i]))) for i in range(len(y)))
        if err <= 1.0:
            t += h
            y = y5
            result.append((t, list(y)))
        h *= min(5.0, 0.9 * (1.0 / err) ** 0.2) if err > 0 else 1.0
        h = max(1e-12, min(h, t_span[1] - t))
    return result


def main():
    parser = argparse.ArgumentParser(description="Adaptive ODE solver using RKF45.")
    parser.add_argument("--func", type=str, required=True, help="ODE function as Python expression, e.g., '[-y[0]]' for dy/dt = -y")
    parser.add_argument("--t0", type=float, default=0.0, help="Initial time")
    parser.add_argument("--tf", type=float, default=1.0, help="Final time")
    parser.add_argument("--y0", type=float, nargs="+", default=[1.0], help="Initial condition(s)")
    parser.add_argument("--rtol", type=float, default=1e-6, help="Relative tolerance")
    parser.add_argument("--atol", type=float, default=1e-9, help="Absolute tolerance")
    args = parser.parse_args()

    def f(t, y): return eval(args.func)
    sol = solve_ivp(f, (args.t0, args.tf), args.y0, args.rtol, args.atol)
    for t, y in sol:
        print(f"{t:.6f} {', '.join(f'{v:.6f}' for v in y)}")


if __name__ == "__main__":
    main()
