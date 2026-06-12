#!/usr/bin/env bash
# deploy_jetson.sh — Déploiement idempotent de SoulSystem sur Jetson AGX
# Usage : bash deploy_jetson.sh [--dev]

set -euo pipefail

echo "=== SoulSystem — Déploiement Jetson AGX ==="

# ── Configuration ──────────────────────────────────────────────────────────────
BIN_SRC="${1:-target/release/soulsystem}"
INSTALL_DIR="/usr/local/bin"
DATA_DIR="/var/lib/soulsystem"
CONFIG_DIR="/opt/soulsystem/config"
LOG_DIR="/var/log/soulsystem"

# ── Vérifications ─────────────────────────────────────────────────────────────
if [ ! -f "$BIN_SRC" ]; then
    echo "❌ Binaire introuvable : $BIN_SRC"
    echo "   Compilez d'abord : cargo build --release"
    exit 1
fi

if [ "$(uname -m)" != "aarch64" ]; then
    echo "⚠️  Attention : la machine cible n'est pas aarch64"
    echo "   (actuel : $(uname -m))"
fi

# ── Installation du binaire ──────────────────────────────────────────────────
sudo install -m 755 "$BIN_SRC" "$INSTALL_DIR/soulsystem"
echo "✅ Binaire installé : $INSTALL_DIR/soulsystem"

# ── Création des répertoires ─────────────────────────────────────────────────
sudo mkdir -p "$DATA_DIR" "$CONFIG_DIR" "$LOG_DIR"
echo "✅ Répertoires créés :"
echo "   - Données  : $DATA_DIR"
echo "   - Config   : $CONFIG_DIR"
echo "   - Logs     : $LOG_DIR"

# ── Copie de la configuration par défaut ─────────────────────────────────────
if [ -f "soulsystem.toml" ]; then
    sudo cp soulsystem.toml "$CONFIG_DIR/"
    echo "✅ Configuration copiée"
fi

# ── Configuration système ────────────────────────────────────────────────────
# Limites mémoire (nécessaire sur Jetson avec 16-32GB RAM partagée)
if [ -f /etc/security/limits.d/ ]; then
    echo "* soft memlock unlimited" | sudo tee /etc/security/limits.d/soulsystem.conf > /dev/null
    echo "* hard memlock unlimited" | sudo tee -a /etc/security/limits.d/soulsystem.conf > /dev/null
    echo "✅ Limites memlock configurées"
fi

# ── Vérification ─────────────────────────────────────────────────────────────
echo ""
echo "=== Vérification ==="
soulsystem --version
echo ""
echo "=== Déploiement terminé ✅ ==="
echo "   Lancez : soulsystem"
echo "   Ou     : soulsystem --dev (avec dashboard)"
