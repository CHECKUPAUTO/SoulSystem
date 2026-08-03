#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)

echo "=== soulsystem-lite ==="
PID=$(pgrep -x soulsystem-lite 2>/dev/null || pgrep -f "target/release/soulsystem" 2>/dev/null || true)
if [ -n "$PID" ]; then
    echo "RUNNING (PID=$PID)"
    echo "$PID" > /tmp/soulsystem-lite.pid 2>/dev/null || true
    curl -sf http://127.0.0.1:9023/health | python3 -m json.tool 2>/dev/null || echo "health endpoint unreachable"
else
    echo "STOPPED"
fi

echo ""
echo "=== CCOS MCP ==="
ps aux | grep "ccos.*mcp" | grep -v grep | awk '{print "RUNNING (PID="$2", cmd="$11" "$12")"}'

echo ""
echo "=== CERVO ==="
if [ -n "$PID" ]; then
    echo "(embedded in soulsystem-lite PID=$PID)"
    curl -sf http://127.0.0.1:9023/cervo/status | python3 -m json.tool 2>/dev/null || echo "cervo endpoint unreachable"
else
    echo "STOPPED (soulsystem-lite not running)"
fi

echo ""
echo "=== SoulSystem Gateway ==="
GW=$(ps aux | grep "soulsystem-gateway" | grep -v grep | awk '{print $2}')
if [ -n "$GW" ]; then
    echo "RUNNING (PID=$GW)"
else
    echo "STOPPED"
fi

echo ""
echo "=== OctaSoma ==="
if ps aux | grep "octasoma-mcp" | grep -v grep > /dev/null 2>&1; then
    OCTA_PID=$(ps aux | grep "octasoma-mcp" | grep -v grep | awk '{print $2}')
    WM_FILE=$(ps aux | grep "octasoma-mcp" | grep -v grep | awk '{print $NF}')
    echo "RUNNING (PID=$OCTA_PID, workspace=$WM_FILE)"
    OCTA_BIN="${OCTASOMA_CMD:-}"
    [ -z "$OCTA_BIN" ] && OCTA_BIN=$(command -v octasoma-mcp 2>/dev/null || true)
    [ -z "$OCTA_BIN" ] && [ -f "$REPO_ROOT/target/release/octasoma-mcp" ] && OCTA_BIN="$REPO_ROOT/target/release/octasoma-mcp"
    [ -z "$OCTA_BIN" ] && [ -f "$REPO_ROOT/octasoma/target/release/octasoma-mcp" ] && OCTA_BIN="$REPO_ROOT/octasoma/target/release/octasoma-mcp"
    if [ -n "$OCTA_BIN" ]; then
        echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"stats","arguments":{}}}' | timeout 3 "$OCTA_BIN" /tmp/octasoma-status.mem 2>/dev/null | python3 -m json.tool 2>/dev/null || echo "(MCP stat probe unavailable)"
    else
        echo "(MCP stat probe unavailable: octasoma-mcp not found)"
    fi
else
    echo "STOPPED"
fi

echo ""
echo "=== Ollama ==="
curl -sf http://127.0.0.1:11434/api/tags > /dev/null 2>&1 && echo "OK ($(curl -sf http://127.0.0.1:11434/api/tags 2>/dev/null | python3 -c "import json,sys;d=json.load(sys.stdin);print(len(d.get('models',[])))") models)" || echo "STOPPED"
