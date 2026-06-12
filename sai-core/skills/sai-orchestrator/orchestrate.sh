#!/bin/bash
# orchestrate.sh — Orchestrateur SAI : plan → exec → verify → log

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/detect-mode.sh"
source "$SCRIPT_DIR/lib/log.sh"

SAI_VERIFY="${SAI_VERIFY:-$HOME/.openclaw/skills/auto-verify/verify.sh}"
SAI_PROJECT_ID="${SAI_PROJECT_ID:-$(date +%s)}"

usage() {
  echo "Usage: $0 --mode [project|interactive] --files N --cmds N [--verify file1,file2,...] [--log-action action] [--log-result result]"
  exit 1
}

MODE=""
FILE_COUNT=0
CMD_COUNT=0
VERIFY_FILES=""
LOG_ACTION=""
LOG_RESULT=""

while [[ $# -gt 0 ]]; do
  case $1 in
    --mode) MODE="$2"; shift 2 ;;
    --files) FILE_COUNT="$2"; shift 2 ;;
    --cmds) CMD_COUNT="$2"; shift 2 ;;
    --verify) VERIFY_FILES="$2"; shift 2 ;;
    --log-action) LOG_ACTION="$2"; shift 2 ;;
    --log-result) LOG_RESULT="$2"; shift 2 ;;
    *) usage ;;
  esac
done

# Auto-detect mode if not provided
if [ -z "$MODE" ]; then
  MODE=$(sai_detect_mode "$FILE_COUNT" "$CMD_COUNT" "")
fi

echo "🔧 [SAI] Mode détecté : $MODE"

# Verify pipeline
if [ -n "$VERIFY_FILES" ]; then
  IFS=',' read -ra FILES <<< "$VERIFY_FILES"
  for f in "${FILES[@]}"; do
    if [ -f "$f" ]; then
      echo "🔍 [SAI] Verify : $f"
      if ! bash "$SAI_VERIFY" "$f"; then
        echo "❌ [SAI] Échec verify sur $f — stop"
        exit 1
      fi
    else
      echo "⚠️ [SAI] Fichier introuvable : $f"
    fi
  done
fi

# Log exec
if [ -n "$LOG_ACTION" ]; then
  sai_log_exec "$LOG_ACTION" "${LOG_RESULT:-✅}"
fi

echo "✅ [SAI] Orchestration terminée"
