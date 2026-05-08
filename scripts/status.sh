#!/bin/bash
# SoulSystem — Status script

echo "═══════════════════════════════════════════════════════════"
echo "🦞 SoulSystem Status"
echo "═══════════════════════════════════════════════════════════"

# OpenClaw-U
if systemctl is-active openclaw-u.service >/dev/null 2>&1; then
    echo "✅ OpenClaw-U     — RUNNING"
    journalctl -u openclaw-u --no-pager -n 1 | grep -oP 'cycle #\d+' | head -1
else
    echo "❌ OpenClaw-U     — STOPPED"
fi

# SoulLink organs
echo ""
echo "🧠 SoulLink Organs:"
for svc in sl13-monolith soul-decision-v13 soullink-gateway; do
    if systemctl is-active "$svc.service" >/dev/null 2>&1; then
        echo "  ✅ $svc"
    else
        echo "  ❌ $svc"
    fi
done

# Ports
echo ""
echo "🌐 Ports:"
for port in 9010 9011 9012 9013 9014 9015 9020 9030 9051 9095 9786; do
    if ss -tlnp | grep -q ":$port "; then
        echo "  ✅ :$port"
    else
        echo "  ❌ :$port"
    fi
done

# Resources
echo ""
echo "📊 Resources:"
echo "  CPU: $(grep 'cpu ' /proc/stat | awk '{usage=($2+$4)*100/($2+$4+$5)} END {printf "%.0f%%", usage}')"
echo "  MEM: $(free | grep Mem | awk '{printf "%.0f%%", $3/$2 * 100.0}')"
echo "  DISK: $(df / | tail -1 | awk '{printf "%s", $5}')"

echo ""
echo "═══════════════════════════════════════════════════════════"
