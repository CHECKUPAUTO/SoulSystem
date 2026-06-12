#!/usr/bin/env python3
"""
Subsidence Monitor - Estimate ground subsidence using multi-temporal SAR interferometry principles.
Simulates the approach from the Indo-US space mission study on extreme subsidence.
"""

import argparse
import numpy as np
import sys


def estimate_subsidence(displacement_series, wavelength=0.23, time_intervals=None):
    """
    Estimate subsidence rate from cumulative displacement measurements.
    Uses linear regression on phase-derived displacements (in meters).
    
    Args:
        displacement_series (np.ndarray): 1D array of displacement values (meters, positive = subsidence).
        wavelength (float): Radar wavelength in meters (default: X-band ~0.23m).
        time_intervals (np.ndarray, optional): Time intervals between measurements in days.
    
    Returns:
        dict: Subsidence rate (m/year), R², and residuals.
    """
    displacement_series = np.asarray(displacement_series)
    n = len(displacement_series)
    if n < 2:
        raise ValueError("At least two displacement measurements required.")
    
    if time_intervals is None:
        # Assume uniform sampling: 12-day repeat cycle (typical for Sentinel-1)
        time_intervals = np.ones(n - 1) * 12.0
    else:
        time_intervals = np.asarray(time_intervals)
        if len(time_intervals) != n - 1:
            raise ValueError("time_intervals must have length n-1.")
    
    # Cumulative time from first measurement
    cum_time = np.concatenate([[0], np.cumsum(time_intervals)]) / 365.25  # years
    
    # Linear regression: displacement = rate * time + intercept
    A = np.vstack([cum_time, np.ones(n)]).T
    coeffs, residuals, rank, s = np.linalg.lstsq(A, displacement_series, rcond=None)
    rate, intercept = coeffs
    
    # Compute R²
    ss_res = np.sum((displacement_series - A @ coeffs) ** 2)
    ss_tot = np.sum((displacement_series - np.mean(displacement_series)) ** 2)
    r_squared = 1 - (ss_res / ss_tot) if ss_tot > 0 else 0.0
    
    return {
        "subsidence_rate_m_per_yr": float(rate),
        "r_squared": float(r_squared),
        "intercept_m": float(intercept),
        "residuals": residuals.tolist() if residuals.size else []
    }


def main():
    parser = argparse.ArgumentParser(
        description="Estimate ground subsidence rate from SAR interferometric measurements."
    )
    parser.add_argument(
        "--displacements",
        nargs="+",
        type=float,
        required=True,
        help="Displacement measurements (meters, positive = subsidence)."
    )
    parser.add_argument(
        "--wavelength",
        type=float,
        default=0.23,
        help="Radar wavelength in meters (default: 0.23 for X-band)."
    )
    parser.add_argument(
        "--time-intervals",
        nargs="+",
        type=float,
        default=None,
        help="Time intervals between measurements in days (default: 12 days each)."
    )
    
    args = parser.parse_args()
    
    try:
        result = estimate_subsidence(
            np.array(args.displacements),
            wavelength=args.wavelength,
            time_intervals=np.array(args.time_intervals) if args.time_intervals else None
        )
        print(f"Subsidence rate: {result['subsidence_rate_m_per_yr']:.4f} m/year")
        print(f"Goodness of fit (R²): {result['r_squared']:.4f}")
        print(f"Intercept: {result['intercept_m']:.4f} m")
        if result['residuals']:
            print(f"Residual sum of squares: {sum(result['residuals']):.6f}")
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
