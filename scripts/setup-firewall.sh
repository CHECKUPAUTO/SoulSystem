#!/bin/bash
# SoulSystem Firewall Setup (nftables)
set -e

echo "🦞 Configuring SoulSystem Firewall (nftables)..."

# Ensure nftables is installed
if ! command -v nft &> /dev/null; then
    echo "❌ nftables not found. Please install it."
    exit 1
fi

# Reset rules
nft flush ruleset

# Create table
nft add table inet filter

# Create chains
nft add chain inet filter input { type filter hook input priority 0 \; policy drop \; }
nft add chain inet filter forward { type filter hook forward priority 0 \; policy drop \; }
nft add chain inet filter output { type filter hook output priority 0 \; policy accept \; }

# 1. Allow established and related traffic
nft add rule inet filter input ct state established,related accept

# 2. Allow loopback interface
nft add rule inet filter input iif lo accept

# 3. Allow SSH (Port 22)
nft add rule inet filter input tcp dport 22 accept

# 4. Allow HNN Mesh ports (9010-9015)
nft add rule inet filter input tcp dport 9010-9015 accept

# 5. Allow Orchestrator (Port 9020)
nft add rule inet filter input tcp dport 9020 accept

# Save rules (distribution specific, typically /etc/nftables.conf)
# nft list ruleset > /etc/nftables.conf

echo "✅ SoulSystem Firewall configured!"
nft list ruleset
