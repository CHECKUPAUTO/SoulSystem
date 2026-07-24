#!/usr/bin/env bash
# SoulSystem — Auto-start script
# Compatible Debian (x86_64) + Jetson AGX (aarch64)
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
SOUL_BIN=""
ARCH=$(uname -m)
OLLAMA_ENDPOINT="${OLLAMA_ENDPOINT:-http://127.0.0.1:11434}"

# Find binary
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
fi

if [ -z "$SOUL_BIN" ]; then
    echo "ERROR: no soulsystem binary found"
    exit 1
fi

# Find CCOS binary
CCOS_BIN=""
if [ -n "${CCOS_CMD:-}" ]; then
    CCOS_BIN="$CCOS_CMD"
elif command -v ccos > /dev/null 2>&1; then
    CCOS_BIN=$(command -v ccos)
elif [ "$ARCH" = "aarch64" ] && [ -f "$REPO_ROOT/ccos/target/aarch64-unknown-linux-gnu/release/ccos" ]; then
    CCOS_BIN="$REPO_ROOT/ccos/target/aarch64-unknown-linux-gnu/release/ccos"
elif [ -f "$REPO_ROOT/target/release/ccos" ]; then
    CCOS_BIN="$REPO_ROOT/target/release/ccos"
elif [ -f "$REPO_ROOT/ccos/target/release/ccos" ]; then
    CCOS_BIN="$REPO_ROOT/ccos/target/release/ccos"
fi

# Already running?
PID_FILE="/tmp/soulsystem-lite.pid"
if [ -f "$PID_FILE" ] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
    exit 0
fi

# Ensure Ollama is up before starting
for i in $(seq 1 30); do
    if curl -sf http://127.0.0.1:11434/api/tags > /dev/null 2>&1; then
        break
    fi
    sleep 1
done

export RUST_LOG="${RUST_LOG:-info}"
export OLLAMA_ENDPOINT
export CCOS_WORKSPACE="${CCOS_WORKSPACE:-/tmp/ccos-soulsystem.ccos}"
[ -n "$CCOS_BIN" ] && export CCOS_CMD="$CCOS_BIN"

echo "[$(date)] Starting soulsystem: $SOUL_BIN" >> /tmp/soulsystem-lite-startup.log
nohup "$SOUL_BIN" > /tmp/soulsystem-lite.log 2>&1 &
PID=$!
echo "$PID" > "$PID_FILE"

# Wait for health
for i in $(seq 1 15); do
    if curl -sf http://127.0.0.1:9023/health > /dev/null 2>&1; then
        echo "[$(date)] OK (PID=$PID)" >> /tmp/soulsystem-lite-startup.log
        exit 0
    fi
    sleep 1
done

echo "[$(date)] FAILED to start" >> /tmp/soulsystem-lite-startup.log
exit 1
