"""Shared test utilities for turbovec integration tests.

Eliminates 7 copies of unit_vector/unit_vectors across test files and
provides a common DeterministicEmbedder base used by both Agno and
LangChain stub embedders.
"""
from __future__ import annotations

import numpy as np


def unit_vector(seed: int, dim: int = 64) -> list[float]:
    """Deterministic unit-norm vector for test vectors."""
    rng = np.random.default_rng(seed)
    v = rng.standard_normal(dim).astype(np.float32)
    v /= np.linalg.norm(v) + 1e-9
    return v.tolist()


def unit_vectors(n: int, dim: int = 128, seed: int = 0) -> np.ndarray:
    """Deterministic batch of unit-norm vectors."""
    rng = np.random.default_rng(seed)
    v = rng.standard_normal((n, dim)).astype(np.float32)
    v /= np.linalg.norm(v, axis=1, keepdims=True) + 1e-9
    return v


class DeterministicEmbedder:
    """Framework-agnostic deterministic embedder base.

    Hashes the input string to seed an RNG, producing a reproducible
    unit-norm vector. Similar strings do not map to similar vectors --
    that's fine for structural tests.
    """

    def __init__(self, dim: int = 64) -> None:
        self.dim = dim

    def _embed(self, text: str) -> list[float]:
        rng = np.random.default_rng(abs(hash(text)) % (2**32))
        v = rng.standard_normal(self.dim).astype(np.float32)
        v /= np.linalg.norm(v) + 1e-9
        return v.tolist()
