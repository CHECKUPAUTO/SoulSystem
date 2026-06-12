#!/usr/bin/env python3
"""
Simulation of a Quantum Key Distribution (QKD) Protocol
Based on arXiv:2605.00358v1 — assumed to propose a QKD variant.

This script demonstrates the core idea: Alice and Bob generate a shared secret key
using quantum states, then estimate the quantum bit error rate (QBER) to detect eavesdropping.
"""

import argparse
import random
import math


def generate_random_bits(n: int) -> list[int]:
    """Generate a list of n random bits (0 or 1)."""
    return [random.randint(0, 1) for _ in range(n)]


def generate_random_bases(n: int) -> list[str]:
    """Generate n random measurement bases: '+' for computational (Z), 'x' for diagonal (X)."""
    return [random.choice(['+', 'x']) for _ in range(n)]


def encode_bit_in_basis(bit: int, basis: str) -> str:
    """Encode a bit in the specified basis using qubit states: |0⟩, |1⟩, |+⟩, |−⟩."""
    if basis == '+':
        return '|0>' if bit == 0 else '|1>'
    else:  # 'x' basis
        return '|+>' if bit == 0 else '|−>'


def measure_qubit(state: str, basis: str) -> int:
    """Measure a qubit state in a given basis, returning 0 or 1."""
    if state == '|0>':
        return 0 if basis == '+' else random.randint(0, 1)
    elif state == '|1>':
        return 1 if basis == '+' else random.randint(0, 1)
    elif state == '|+>':
        return 0 if basis == 'x' else random.randint(0, 1)
    elif state == '|−>':
        return 1 if basis == 'x' else random.randint(0, 1)
    raise ValueError(f"Unknown state: {state}")


def estimate_qber(alice_bits: list[int], bob_bits: list[int], sifted_indices: list[int]) -> float:
    """Estimate Quantum Bit Error Rate over sifted key positions."""
    if not sifted_indices:
        return 0.0
    errors = sum(1 for i in sifted_indices if alice_bits[i] != bob_bits[i])
    return errors / len(sifted_indices)


def main():
    parser = argparse.ArgumentParser(description="QKD Protocol Simulator (arXiv:2605.00358v1)")
    parser.add_argument('--n-qubits', type=int, default=100, help="Number of qubits to simulate (default: 100)")
    parser.add_argument('--eavesdropper', action='store_true', help="Simulate an eavesdropper (Eve)")
    parser.add_argument('--seed', type=int, default=None, help="Random seed for reproducibility")
    args = parser.parse_args()

    if args.seed is not None:
        random.seed(args.seed)

    n = args.n_qubits

    # Alice generates bits and bases
    alice_bits = generate_random_bits(n)
    alice_bases = generate_random_bases(n)

    # Encode bits into qubits
    qubits = [encode_bit_in_basis(b, a) for b, a in zip(alice_bits, alice_bases)]

    # Eve intercepts if enabled
    if args.eavesdropper:
        eve_bases = generate_random_bases(n)
        # Eve measures and re-encodes
        qubits = [encode_bit_in_basis(measure_qubit(q, e), e) for q, e in zip(qubits, eve_bases)]

    # Bob measures in random bases
    bob_bases = generate_random_bases(n)
    bob_bits = [measure_qubit(q, b) for q, b in zip(qubits, bob_bases)]

    # Sifting: keep only bits where Alice and Bob used the same basis
    sifted_indices = [i for i in range(n) if alice_bases[i] == bob_bases[i]]
    alice_sifted = [alice_bits[i] for i in sifted_indices]
    bob_sifted = [bob_bits[i] for i in sifted_indices]

    # Estimate QBER
    qber = estimate_qber(alice_bits, bob_bits, sifted_indices)

    # Output summary
    print(f"QKD Simulation Summary (n={n})")
    print(f"  Sifted key length: {len(sifted_indices)}")
    print(f"  Estimated QBER: {qber:.2%}")
    print(f"  Eavesdropper present: {args.eavesdropper}")
    print(f"  Secure key possible: {'Yes' if qber < 0.11 else 'No (too many errors)'}")


if __name__ == "__main__":
    main()
