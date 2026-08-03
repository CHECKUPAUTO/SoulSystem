#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

# Source .env if it exists
if [ -f ".env" ]; then
    # shellcheck disable=SC1091
    source .env
fi

# Check required env vars
if [ -z "${AVID_API_TOKEN:-}" ]; then
    echo "ERROR: AVID_API_TOKEN is not set. Create a .env file or export it."
    echo "  Example: cp .env.example .env && vim .env"
    exit 78
fi

if [ -z "${AVID_HOME:-}" ]; then
    echo "ERROR: AVID_HOME is not set. Example: export AVID_HOME=/var/lib/avid"
    exit 78
fi

if [ ! -d "$AVID_HOME" ]; then
    echo "Creating AVID_HOME directory: $AVID_HOME"
    mkdir -p "$AVID_HOME"
fi

if [ -z "${OLLAMA_URL:-}" ]; then
    echo "ERROR: OLLAMA_URL is not set."
    exit 78
fi

if [ -z "${OLLAMA_MODEL:-}" ]; then
    echo "ERROR: OLLAMA_MODEL is not set."
    exit 78
fi

MODE="${1:-server}"

echo "=== AVID + SoulSystem ($MODE) ==="
echo "Port: ${AVID_PORT:-8088}"
echo "Home: $AVID_HOME"
echo "Ollama: $OLLAMA_URL ($OLLAMA_MODEL)"
echo ""

if [ "$MODE" == "tui" ]; then
    exec cargo run --release -p avid-tui
else
    exec cargo run --release -p avid-server
fi
