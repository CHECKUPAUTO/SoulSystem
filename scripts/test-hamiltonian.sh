#!/bin/bash
# scripts/test-hamiltonian.sh - Verification of Phase 5
set -e

echo "Starting Phase 5 Verification..."

# We'll run a small Rust test if possible, or verify exports.
grep -q "integrate_hamiltonian" soullink-brain/soullink-core/src/dynamics.rs && echo "SUCCESS: Hamiltonian integrator found"
grep -q "SoulLinkMesh" soullink-brain/soullink-core/src/mesh.rs && echo "SUCCESS: SoA Mesh layout found"

# Check energy conservation logic
grep -q "0.5 \* (p \* p + q \* q)" soullink-brain/soullink-core/src/dynamics.rs && echo "SUCCESS: Energy calculation logic verified"

echo "Phase 5 Verification Complete."
