#!/bin/bash
set -e

OLLAMA_URL="${SOULSYSTEM_EVO_OLLAMA_URL:-http://localhost:11434}"
MODEL_FAST="${SOULSYSTEM_EVO_MODEL_FAST:-qwen3.5:4b}"
MODEL_DEEP="${SOULSYSTEM_EVO_MODEL_DEEP:-llama3:8b}"
MODEL_EMBED="${SOULSYSTEM_EVO_MODEL_EMBED:-nomic-embed-text}"

echo "╔═══════════════════════════════════════════════╗"
echo "║  SoulSystem Evolution v0.2 — Démarrage          ║"
echo "╚═══════════════════════════════════════════════╝"
echo ""
echo "  Ollama    : $OLLAMA_URL"
echo "  Fast      : $MODEL_FAST"
echo "  Deep      : $MODEL_DEEP"
echo "  Embed     : $MODEL_EMBED"
echo ""

# ── Attendre Ollama ──────────────────────────
echo "⏳ Vérification Ollama..."
MAX_RETRIES=30
RETRY=0
while [ $RETRY -lt $MAX_RETRIES ]; do
    if curl -sf "${OLLAMA_URL}/api/tags" > /dev/null 2>&1; then
        echo "✅ Ollama connecté"
        break
    fi
    RETRY=$((RETRY + 1))
    echo "   Tentative $RETRY/$MAX_RETRIES..."
    sleep 2
done

if [ $RETRY -eq $MAX_RETRIES ]; then
    echo "⚠️  Ollama non disponible — mode dégradé"
fi

# ── Vérifier les 3 modèles ──────────────────
TAGS=$(curl -sf "${OLLAMA_URL}/api/tags" 2>/dev/null || echo "{}")
for MODEL in "$MODEL_FAST" "$MODEL_DEEP" "$MODEL_EMBED"; do
    if echo "$TAGS" | grep -q "$MODEL"; then
        echo "✅ Modèle $MODEL disponible"
    else
        echo "⚠️  Modèle $MODEL non trouvé — pull en cours..."
        curl -sf "${OLLAMA_URL}/api/pull" -d "{\"name\":\"$MODEL\"}" > /dev/null 2>&1 || \
            echo "   Pull échoué pour $MODEL"
    fi
done

echo ""
echo "🚀 Lancement SoulSystem Evolution v0.2..."
echo "──────────────────────────────────────"

exec ./soulsystem
