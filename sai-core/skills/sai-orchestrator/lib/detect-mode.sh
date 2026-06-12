#!/bin/bash
# detect-mode.sh — Détecte si une tâche doit basculer en mode projet

set -euo pipefail

sai_detect_mode() {
  local file_count="${1:-0}"
  local cmd_count="${2:-0}"
  local prompt="${3:-}"

  local triggers="crée un système|construis|refactor|migrer|architecte|implémente|développe|créer un"

  if [ "$file_count" -gt 3 ] || [ "$cmd_count" -gt 2 ]; then
    echo "project"
    return 0
  fi

  if echo "$prompt" | grep -qiE "$triggers"; then
    echo "project"
    return 0
  fi

  echo "interactive"
}
