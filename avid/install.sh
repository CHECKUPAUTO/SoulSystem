#!/usr/bin/env bash
set -euo pipefail

echo "=== AVID + SoulSystem — Install ==="

# ── System dependencies ────────────────────────────────────────────────
echo "[1/4] Installing system packages..."
apt-get update -qq
apt-get install -y -qq --no-install-recommends \
    build-essential \
    pkg-config \
    python3 \
    sqlite3 \
    curl \
    git \
    ca-certificates

# Ensure python3 is on PATH
if ! command -v python3 &>/dev/null; then
    echo "ERROR: python3 not found after install"
    exit 1
fi

# ── Rust toolchain 1.88 ─────────────────────────────────────────────────
echo "[2/4] Installing Rust toolchain 1.88..."

if ! command -v rustup &>/dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.88
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
fi

# Ensure the 1.88 toolchain is installed
rustup toolchain install 1.88 2>/dev/null || true
rustup default 1.88 2>/dev/null || true

# ── Verify Rust ─────────────────────────────────────────────────────────
echo "[3/4] Verifying Rust installation..."
rustc --version
cargo --version

# ── Build workspace ─────────────────────────────────────────────────────
echo "[4/4] Building workspace (release)..."
cd "$(dirname "$0")"
cargo build --release --workspace 2>&1

echo ""
echo "=== Install complete ==="
echo "Binary: ./target/release/avid-server"
echo "Next: source .env && bash run.sh"
