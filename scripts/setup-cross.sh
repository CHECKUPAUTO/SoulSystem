#!/bin/bash
# scripts/setup-cross.sh - Setup cross-compilation environment
set -e

echo "Setting up aarch64 cross-compilation environment..."
apt-get update && apt-get install -y \
    gcc-aarch64-linux-gnu \
    libc6-dev-arm64-cross \
    qemu-user \
    || echo "Warning: Some packages could not be installed. Cross-compilation might fail if not in a full environment."

rustup target add aarch64-unknown-linux-gnu || true
echo "Setup complete."
