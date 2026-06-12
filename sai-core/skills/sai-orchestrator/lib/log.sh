#!/bin/bash
# log.sh — Fonctions de logging pour le SAI

set -euo pipefail

SAI_MEMORY_DIR="${SAI_MEMORY_DIR:-$HOME/memory}"

sai_log_plan() {
  local project_id="$1"
  local step="$2"
  local deps="$3"
  local risk="$4"
  local file="$SAI_MEMORY_DIR/$(date +%Y-%m-%d).md"
  mkdir -p "$SAI_MEMORY_DIR"
  echo "| $step | $deps | $risk | ✅ |" >> "$file"
}

sai_log_exec() {
  local action="$1"
  local result="$2"
  local file="$SAI_MEMORY_DIR/$(date +%Y-%m-%d).md"
  echo "### $(date +%H:%M) — $action" >> "$file"
  echo "- Résultat : $result" >> "$file"
}

sai_log_reflection() {
  local pre_action="$1"
  local post_result="$2"
  local lesson="$3"
  local file="$SAI_MEMORY_DIR/$(date +%Y-%m-%d).md"
  echo "## reflection — $(date +%H:%M)" >> "$file"
  echo "- pré-action : $pre_action" >> "$file"
  echo "- post-action : $post_result" >> "$file"
  echo "- leçon : $lesson" >> "$file"
}
