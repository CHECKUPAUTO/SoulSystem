#!/bin/bash
# SoulSystem — Deploy script
set -e

echo "🦞 Deploying SoulSystem..."

# Backup current binaries
sudo cp /usr/local/bin/soulsystem-gateway-u /usr/local/bin/soulsystem-gateway-u.bak.$(date +%Y%m%d_%H%M%S) 2>/dev/null || true

# Deploy SoulSystem gateway
sudo cp /root/SoulSystem/soulsystem-gateway/target/release/soulsystem-gateway /usr/local/bin/soulsystem-gateway-u
sudo systemctl restart soulsystem-gateway-u

# Deploy SoulLink binaries
for binary in soullink-node soullink-monolith soullink-orchestrator; do
    if [ -f "/root/SoulSystem/soullink-organs/target/release/$binary" ]; then
        sudo cp "/root/SoulSystem/soullink-organs/target/release/$binary" /usr/local/bin/
    fi
done

# Restart services
sudo systemctl restart sl13-monolith.service
sudo systemctl restart soulsystem-gateway-u.service

echo "✅ SoulSystem deployed!"
echo "Status:"
./scripts/status.sh
