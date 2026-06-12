#!/usr/bin/env python3
"""
Diffusion Sampler - Simple denoising diffusion sampler without trained model.
Implements reverse-time sampling using a Gaussian approximation of the
forward process, as inspired by DDPM (Ho et al., 2020) and related works.
"""

import argparse
import numpy as np
import math

def get_beta_schedule(num_timesteps: int, beta_start: float = 1e-4, beta_end: float = 0.02) -> np.ndarray:
    """Linear beta schedule as in DDPM."""
    return np.linspace(beta_start, beta_end, num_timesteps)

def compute_alpha_bar(beta_t: np.ndarray) -> np.ndarray:
    """Compute cumulative product of (1 - beta_t) -> α̅_t."""
    alpha = 1.0 - beta_t
    return np.cumprod(alpha)

def sample_diffusion(
    num_samples: int,
    num_timesteps: int = 1000,
    shape: tuple = (2,),
    seed: int = 42
) -> np.ndarray:
    """
    Generate samples using reverse-time diffusion without a trained model.
    Uses the analytical Gaussian approximation of q(x_{t-1}|x_t, x_0) with x_0 ~ N(0, I).
    """
    np.random.seed(seed)
    
    # Forward process parameters
    betas = get_beta_schedule(num_timesteps)
    alphas = 1.0 - betas
    alpha_bars = compute_alpha_bar(betas)
    
    # Start from pure noise
    x_t = np.random.randn(num_samples, *shape)
    
    # Reverse sampling loop (simplified DDPM sampler)
    for t in reversed(range(num_timesteps)):
        # Predict x_0 using the analytical formula for Gaussian prior
        # x_0 ≈ (x_t / sqrt(α̅_t) - sqrt((1 - α̅_t)/α̅_t) * ε_pred) 
        # Here we assume ε_pred = 0 (prior mean), giving x_0 ≈ x_t / sqrt(α̅_t)
        alpha_bar_t = alpha_bars[t]
        x0_pred = x_t / np.sqrt(alpha_bar_t)
        
        # Compute mean of q(x_{t-1}|x_t, x_0_pred)
        alpha_t = alphas[t]
        beta_t = betas[t]
        mean_coef1 = np.sqrt(alpha_t) * (1 - alpha_bar_t) / np.sqrt(1 - alpha_bar_t * alpha_t)
        mean_coef2 = np.sqrt(1 - alpha_t) * np.sqrt(alpha_bar_t) / np.sqrt(1 - alpha_bar_t * alpha_t)
        
        mean = mean_coef1 * x0_pred + mean_coef2 * x_t
        
        # Add noise only if t > 0
        if t > 0:
            noise = np.random.randn(num_samples, *shape)
            std = np.sqrt(beta_t)
            x_t = mean + std * noise
        else:
            x_t = mean
    
    return x_t

def main():
    parser = argparse.ArgumentParser(description="Generate samples using diffusion without a trained model.")
    parser.add_argument("--num_samples", type=int, default=5, help="Number of samples to generate")
    parser.add_argument("--num_steps", type=int, default=50, help="Number of diffusion steps (lower for speed)")
    parser.add_argument("--seed", type=int, default=42, help="Random seed")
    parser.add_argument("--shape", type=str, default="2", help="Shape of each sample (e.g., 2 or 3,784)")
    args = parser.parse_args()
    
    shape = tuple(map(int, args.shape.split(","))) if "," in args.shape else (int(args.shape),)
    samples = sample_diffusion(
        num_samples=args.num_samples,
        num_timesteps=args.num_steps,
        shape=shape,
        seed=args.seed
    )
    
    print("Generated samples (shape {}):".format(samples.shape))
    print(samples)

if __name__ == "__main__":
    main()
