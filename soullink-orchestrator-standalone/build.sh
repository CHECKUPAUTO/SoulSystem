#!/bin/bash
# build.sh - Build script for SoulLink Orchestrator v3

set -e

echo "🔨 Building SoulLink Orchestrator v3 (Rust)..."

# Check Rust
if ! command -v rustc &> /dev/null; then
    echo "❌ Rust not found. Install with: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

echo "✓ Rust version: $(rustc --version)"

# Build
echo "→ Building release binary..."
cargo build --release

# Check if binary was created
if [ -f "target/release/soullink-orchestrator" ]; then
    echo "✓ Binary created: target/release/soullink-orchestrator"
    echo "✓ Size: $(du -h target/release/soullink-orchestrator | cut -f1)"
else
    echo "❌ Build failed"
    exit 1
fi

echo ""
echo "🚀 Installation:"
echo "   sudo cp target/release/soullink-orchestrator /usr/local/bin/"
echo "   sudo chmod +x /usr/local/bin/soullink-orchestrator"
echo ""
echo "🎮 Run:"
echo "   soullink-orchestrator"
echo "   soullink-orchestrator 9020 /mnt/nvme/soullink_brain"
