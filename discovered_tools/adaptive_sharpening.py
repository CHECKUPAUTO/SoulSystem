#!/usr/bin/env python3
"""
Adaptive Sharpening Tool

Démonstration d'un filtre adaptatif de netteté pour réduire les artefacts de sur-échantillonnage.
Basé sur les principes du paper arXiv:2605.00329v1.
"""

import argparse
import numpy as np
from PIL import Image
import sys


def gaussian_kernel(size: int, sigma: float) -> np.ndarray:
    """Génère un noyau gaussien 2D."""
    ax = np.linspace(-(size // 2), size // 2, size)
    xx, yy = np.meshgrid(ax, ax)
    kernel = np.exp(-(xx**2 + yy**2) / (2 * sigma**2))
    return kernel / kernel.sum()


def adaptive_sharpen(image: np.ndarray, radius: int = 3, sigma: float = 1.0, amount: float = 0.5) -> np.ndarray:
    """
    Applique un filtre de netteté adaptatif.
    
    Args:
        image: Image en format numpy array (H, W) ou (H, W, C)
        radius: Rayon du noyau local
        sigma: Écart-type du noyau gaussien
        amount: Intensité de la sharpening (0 à 1)
    
    Returns:
        Image nettoyée en array numpy
    """
    if image.ndim == 3:
        # Gestion des images RGB
        result = np.empty_like(image)
        for c in range(image.shape[2]):
            result[:, :, c] = _sharpen_channel(image[:, :, c], radius, sigma, amount)
        return result
    else:
        return _sharpen_channel(image, radius, sigma, amount)


def _sharpen_channel(channel: np.ndarray, radius: int, sigma: float, amount: float) -> np.ndarray:
    """Applique le filtre à un seul canal."""
    kernel = gaussian_kernel(2 * radius + 1, sigma)
    blurred = np.zeros_like(channel)
    pad = radius
    padded = np.pad(channel, pad, mode='reflect')
    
    for i in range(-pad, pad + 1):
        for j in range(-pad, pad + 1):
            blurred += padded[pad + i:pad + i + channel.shape[0], pad + j:pad + j + channel.shape[1]] * kernel[i + pad, j + pad]
    
    # Calcul du masque adaptatif basé sur la variance locale
    local_var = np.zeros_like(channel)
    for i in range(-pad, pad + 1):
        for j in range(-pad, pad + 1):
            local_var += (padded[pad + i:pad + i + channel.shape[0], pad + j:pad + j + channel.shape[1]] - blurred)**2
    
    # Normalisation
    threshold = np.percentile(local_var, 95)
    mask = np.clip(local_var / (threshold + 1e-6), 0, 1)
    
    return channel + amount * (channel - blurred) * mask


def main():
    parser = argparse.ArgumentParser(description="Netteté adaptative pour images")
    parser.add_argument("input", help="Chemin vers l'image d'entrée")
    parser.add_argument("output", help="Chemin vers l'image de sortie")
    parser.add_argument("--radius", type=int, default=3, help="Rayon du noyau (défaut: 3)")
    parser.add_argument("--sigma", type=float, default=1.0, help="Sigma gaussien (défaut: 1.0)")
    parser.add_argument("--amount", type=float, default=0.5, help="Intensité de la netteté (0-1, défaut: 0.5)")
    args = parser.parse_args()
    
    try:
        img = Image.open(args.input).convert('RGB')
        arr = np.array(img, dtype=np.float32) / 255.0
        result = adaptive_sharpen(arr, args.radius, args.sigma, args.amount)
        result = np.clip(result * 255, 0, 255).astype(np.uint8)
        Image.fromarray(result).save(args.output)
        print(f"Image sauvegardée: {args.output}")
    except Exception as e:
        print(f"Erreur: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()