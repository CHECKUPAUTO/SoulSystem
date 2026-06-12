#!/bin/bash
# SoulSystem — Deploy script
set -e

echo "🦞 Deploying SoulSystem..."

# Backup current binaries
sudo cp /usr/local/bin/openclaw-u /usr/local/bin/openclaw-u.bak.$(date +%Y%m%d_%H%M%S) 2>/dev/null || true

# Deploy OpenClaw-U
sudo cp /root/SoulSystem/openclaw-u/target/release/openclaw-u /usr/local/bin/openclaw-u
sudo systemctl restart openclaw-u

# Deploy SoulLink binaries
for binary in soullink-node soullink-monolith soullink-orchestrator; do
    if [ -f "/root/SoulSystem/soullink-organs/target/release/$binary" ]; then
        sudo cp "/root/SoulSystem/soullink-organs/target/release/$binary" /usr/local/bin/
    fi
done

# Restart services
sudo systemctl restart sl13-monolith.service
sudo systemctl restart openclaw-u.service

echo "✅ SoulSystem deployed!"
echo "Status:"
./scripts/status.sh
