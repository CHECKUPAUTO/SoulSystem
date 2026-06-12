#!/bin/bash
# =============================================================================
# SoulSystem Firewall Setup (nftables)
# Usage: sudo ./scripts/setup-firewall.sh [--persist]
#   --persist   Save rules to /etc/nftables.conf and enable systemd service
# =============================================================================
set -euo pipefail

PERSIST=false
if [[ "${1:-}" == "--persist" ]]; then
    PERSIST=true
fi

echo "🦞 Configuring SoulSystem Firewall (nftables)..."

if ! command -v nft &> /dev/null; then
    echo "❌ nftables not found. Please install it: apt install nftables"
    exit 1
fi

# Reset rules
nft flush ruleset

# ── Table for IPv4+IPv6 ────────────────────────────────────────────
nft add table inet filter

# ── Sets for allowed ports ─────────────────────────────────────────
nft add set inet filter allowed_tcp_in { type inet_service \; flags interval \; }
nft add set inet filter allowed_tcp_out { type inet_service \; flags interval \; }
nft add set inet filter allowed_udp_out { type inet_service \; }

# Populate inbound allowed ports (SoulSystem services)
nft add element inet filter allowed_tcp_in { 22, 80, 443, 8080, 777 }
nft add element inet filter allowed_tcp_in { 9010-9015, 9020, 9030, 9051, 9095, 9786 }
nft add element inet filter allowed_tcp_in { 11434, 18890 }

# Populate outbound allowed ports
nft add element inet filter allowed_tcp_out { 22, 80, 443, 53 }
nft add element inet filter allowed_tcp_out { 11434, 9010-9015, 9020, 9030, 9051, 9095 }
nft add element inet filter allowed_udp_out { 53, 123 }

# ── Chains ──────────────────────────────────────────────────────────
nft add chain inet filter input { type filter hook input priority 0 \; policy drop \; }
nft add chain inet filter forward { type filter hook forward priority 0 \; policy drop \; }
nft add chain inet filter output { type filter hook output priority 0 \; policy drop \; }

# ── INPUT RULES ────────────────────────────────────────────────────

# Allow established/related (return traffic)
nft add rule inet filter input ct state established,related accept

# Allow loopback
nft add rule inet filter input iif lo accept

# Allow ICMP (ping, path MTU discovery)
nft add rule inet filter input ip protocol icmp accept
nft add rule inet filter input ip6 nexthdr ipv6-icmp accept

# Allow SSH
nft add rule inet filter input tcp dport 22 accept

# Allow known service ports
nft add rule inet filter input tcp dport @allowed_tcp_in accept

# Log dropped input packets (rate-limited)
nft add rule inet filter input limit rate 3/minute burst 5 packets log prefix "FW-INPUT-DROP: " level info
nft add rule inet filter input reject

# ── FORWARD RULES ──────────────────────────────────────────────────
# Log and drop all forwarded traffic
nft add rule inet filter forward limit rate 3/minute burst 5 packets log prefix "FW-FORWARD-DROP: " level info
nft add rule inet filter forward reject

# ── OUTPUT RULES ──────────────────────────────────────────────────

# Allow established/related
nft add rule inet filter output ct state established,related accept

# Allow loopback
nft add rule inet filter output oif lo accept

# Allow ICMP out
nft add rule inet filter output ip protocol icmp accept
nft add rule inet filter output ip6 nexthdr ipv6-icmp accept

# Allow UDP (DNS, NTP)
nft add rule inet filter output udp dport @allowed_udp_out accept

# Allow TCP to known ports (Ollama, brain mesh, etc.)
nft add rule inet filter output tcp dport @allowed_tcp_out accept

# Log dropped output packets (rate-limited)
nft add rule inet filter output limit rate 3/minute burst 5 packets log prefix "FW-OUTPUT-DROP: " level info
nft add rule inet filter output reject

# ── Persist ──────────────────────────────────────────────────────────
if $PERSIST; then
    echo "💾 Saving rules to /etc/nftables.conf..."
    nft list ruleset > /etc/nftables.conf
    chmod 600 /etc/nftables.conf

    if systemctl is-enabled nftables &>/dev/null; then
        systemctl restart nftables
        echo "✅ nftables service restarted and enabled on boot"
    else
        echo "⚠️  nftables systemd service not enabled. Run: systemctl enable --now nftables"
    fi
fi

echo "✅ SoulSystem Firewall configured!"
nft list ruleset
