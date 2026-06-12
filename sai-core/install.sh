#!/bin/bash
# install.sh — Installe SAI sur une nouvelle machine

set -euo pipefail

REPO_URL="https://github.com/CHECKUPAUTO/sai-core.git"
INSTALL_DIR="${1:-$HOME/.sai-core}"

echo "🔧 Installation SAI v1.0"
echo "   Répertoire : $INSTALL_DIR"
echo ""

# Clone
if [ -d "$INSTALL_DIR/.git" ]; then
  echo "📦 Mise à jour du repo existant..."
  cd "$INSTALL_DIR" && git pull
else
  echo "📦 Clonage..."
  git clone "$REPO_URL" "$INSTALL_DIR"
fi

cd "$INSTALL_DIR"

# Hooks
if [ -d ".git" ]; then
  echo "🔗 Installation des hooks git..."
  git config core.hooksPath .githooks
fi

# Vérification dépendances
echo ""
echo "🔍 Vérification des dépendances :"

for cmd in bash python3 node git; do
  if command -v "$cmd" >/dev/null 2>&1; then
    echo "  ✅ $cmd"
  else
    echo "  ⚠️ $cmd — optionnel ou manquant"
  fi
done

# Alias suggérés
echo ""
echo "💡 Alias suggérés (ajoutez à ~/.bashrc) :"
echo "  alias sai-verify='$INSTALL_DIR/skills/auto-verify/verify.sh'"
echo "  alias sai-orchestrate='bash $INSTALL_DIR/skills/sai-orchestrator/orchestrate.sh'"
echo "  alias sai='cd $INSTALL_DIR && make'"

echo ""
echo "✅ SAI installé dans $INSTALL_DIR"
echo "   cd $INSTALL_DIR && make help"
