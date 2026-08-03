#!/bin/bash
# SoulSystem — Build script
set -e

echo "🦞 Building SoulSystem..."

# Build SoulSystem gateway
echo "🔧 Building SoulSystem gateway..."
cd /root/SoulSystem/soulsystem-gateway
cargo build --release

# Build soullink-organs
echo "🔧 Building SoulLink organs..."
cd /root/SoulSystem/soullink-organs
cargo build --release

# Build soullink-brain workspace
echo "🔧 Building SoulLink brain workspace..."
cd /root/SoulSystem/soullink-brain
cargo build --release

echo "✅ SoulSystem build complete!"
