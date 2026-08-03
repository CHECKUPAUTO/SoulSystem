#!/usr/bin/env bash
# Start or attach to the SoulSystem tmux session
# Compatible Debian + Jetson AGX (multi-arch)
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
SESSION="soulsystem"
CCOS_BIN="${CCOS_CMD:-}"
OLLAMA_ENDPOINT="${OLLAMA_ENDPOINT:-http://127.0.0.1:11434}"
OCTA_BIN="${OCTASOMA_CMD:-}"
OCTA_STORE="${OCTASOMA_STORE:-${HOME:-$REPO_ROOT}/.soulsystem/workspace/octasoma/octasoma.mem}"

# Detect architecture
ARCH=$(uname -m)
echo "=== SoulSystem Session (${ARCH}) ==="

# Find soulsystem binary
if [ -n "${SOULSYSTEM_CMD:-}" ]; then
    SOUL_BIN="$SOULSYSTEM_CMD"
elif command -v soulsystem-lite > /dev/null 2>&1; then
    SOUL_BIN=$(command -v soulsystem-lite)
elif command -v soulsystem > /dev/null 2>&1; then
    SOUL_BIN=$(command -v soulsystem)
elif [ -f "$REPO_ROOT/target/release/soulsystem-lite" ]; then
    SOUL_BIN="$REPO_ROOT/target/release/soulsystem-lite"
elif [ -f "$REPO_ROOT/target/release/soulsystem" ]; then
    SOUL_BIN="$REPO_ROOT/target/release/soulsystem"
else
    echo "ERROR: no soulsystem binary found"
    exit 1
fi

# Resolve optional component binaries.
[ -z "$CCOS_BIN" ] && CCOS_BIN=$(command -v ccos 2>/dev/null || true)
[ -z "$CCOS_BIN" ] && [ "$ARCH" = "aarch64" ] && [ -f "$REPO_ROOT/ccos/target/aarch64-unknown-linux-gnu/release/ccos" ] && CCOS_BIN="$REPO_ROOT/ccos/target/aarch64-unknown-linux-gnu/release/ccos"
[ -z "$CCOS_BIN" ] && [ -f "$REPO_ROOT/target/release/ccos" ] && CCOS_BIN="$REPO_ROOT/target/release/ccos"
[ -z "$CCOS_BIN" ] && [ -f "$REPO_ROOT/ccos/target/release/ccos" ] && CCOS_BIN="$REPO_ROOT/ccos/target/release/ccos"

[ -z "$OCTA_BIN" ] && OCTA_BIN=$(command -v octasoma-mcp 2>/dev/null || true)
[ -z "$OCTA_BIN" ] && [ -f "$REPO_ROOT/target/release/octasoma-mcp" ] && OCTA_BIN="$REPO_ROOT/target/release/octasoma-mcp"
[ -z "$OCTA_BIN" ] && [ -f "$REPO_ROOT/octasoma/target/release/octasoma-mcp" ] && OCTA_BIN="$REPO_ROOT/octasoma/target/release/octasoma-mcp"

printf -v REPO_ROOT_Q '%q' "$REPO_ROOT"
printf -v SOUL_BIN_Q '%q' "$SOUL_BIN"
printf -v OLLAMA_ENDPOINT_Q '%q' "$OLLAMA_ENDPOINT"
printf -v STATUS_SCRIPT_Q '%q' "$SCRIPT_DIR/status.sh"
printf -v OCTA_BIN_Q '%q' "$OCTA_BIN"
printf -v OCTA_STORE_Q '%q' "$OCTA_STORE"
CCOS_ENV=""
[ -n "$CCOS_BIN" ] && printf -v CCOS_ENV 'CCOS_CMD=%q ' "$CCOS_BIN"

if tmux has-session -t "$SESSION" 2>/dev/null; then
    echo "Attaching to existing session..."
    tmux attach-session -t "$SESSION"
    exit 0
fi

tmux new-session -d -s "$SESSION" -n "soulsystem"

# Window 0: soulsystem (CERVO + CCOS)
tmux send-keys -t "$SESSION:0" \
    "cd ${REPO_ROOT_Q} && \
     RUST_LOG=info \
     OLLAMA_ENDPOINT=${OLLAMA_ENDPOINT_Q} \
     ${CCOS_ENV}\
     CCOS_WORKSPACE=/tmp/ccos-soulsystem.ccos \
     ${SOUL_BIN_Q}" Enter

# Window 1: gateway (Debian only — uses npm-distributed soulsystem-gateway)
if command -v soulsystem-gateway &>/dev/null; then
    tmux new-window -t "$SESSION" -n "gateway"
    tmux send-keys -t "$SESSION:1" \
        "soulsystem-gateway --port 18890" Enter
fi

# Window 2: monitor/logs
tmux new-window -t "$SESSION" -n "monitor"
tmux send-keys -t "$SESSION:2" \
    "watch -n 10 ${STATUS_SCRIPT_Q}" Enter

# Window 3: octasoma fractal memory
if [ -n "$OCTA_BIN" ]; then
    mkdir -p "$(dirname -- "$OCTA_STORE")"
    tmux new-window -t "$SESSION" -n "octasoma"
    tmux send-keys -t "$SESSION:3" \
        "OLLAMA_ENDPOINT=${OLLAMA_ENDPOINT_Q} \
         ${OCTA_BIN_Q} ${OCTA_STORE_Q}" Enter
fi

# Window 4: shell (or window 3 if no octasoma)
SHELL_WIN=3
[ -n "$OCTA_BIN" ] && SHELL_WIN=4
tmux new-window -t "$SESSION" -n "shell"
tmux send-keys -t "$SESSION:${SHELL_WIN}" "cd ${REPO_ROOT_Q} && echo 'SoulSystem shell ready'" Enter

echo ""
echo "=== Session '$SESSION' created ==="
echo "Attach: tmux attach -t soulsystem"
echo "Windows: soulsystem | gateway | monitor | octasoma | shell"
echo ""
