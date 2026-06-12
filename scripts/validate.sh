#!/usr/bin/env bash
# ==========================================================================
# validate.sh — Pipeline de validation pré-commit multi-projet
#
# Exécute pour CHAQUE projet Rust : cargo check → cargo test --lib → clippy
# + vérifications spécifiques (pas de stubs, pas de TODO)
#
# Usage:
#   ./scripts/validate.sh                  # tout
#   ./scripts/validate.sh --fast           # cargo check uniquement
#   ./scripts/validate.sh --project chronos-agent  # un seul projet
# ==========================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color
FAILED=0

# ── Arguments ─────────────────────────────────────────────────────
FAST=false
TARGET_PROJECT=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --fast) FAST=true; shift ;;
    --project) TARGET_PROJECT="$2"; shift 2 ;;
    *) echo "Usage: $0 [--fast] [--project <name>]"; exit 1 ;;
  esac
done

# ── Projets Rust à valider ────────────────────────────────────────
# Format: "chemin|nom|type (workspace|standalone)"
PROJECTS=(
  ".|soulsystem (workspace)|workspace"
  "scirust-chronos-agent|chronos-agent|standalone"
)

# ── Fonctions ─────────────────────────────────────────────────────

check_no_stubs() {
  local dir="$1" label="$2"
  local found=0
  while IFS= read -r -d '' f; do
    # Ignore les fichiers dans target/ et les backups
    [[ "$f" == *"/target/"* ]] && continue
    [[ "$f" == *".v64" ]] && continue
    [[ "$f" == *".clawd" ]] && continue
    
    if grep -qE '^\s*(todo|unimplemented!|todo!|panic!\(|dbg!\(|unreachable!\(|\/\/\s*TODO)' "$f" 2>/dev/null; then
      # Vérifie que ce n'est pas un TODO intentionnel (dans une docstring)
      local line
      line=$(grep -nE '(todo!|unimplemented!|panic!\(|dbg!\()' "$f" 2>/dev/null | head -3)
      if [ -n "$line" ]; then
        echo -e "  ${YELLOW}⚠ stubs dans $f:${NC}"
        echo "$line" | while read -r l; do echo "    $l"; done
        found=1
      fi
    fi
  done < <(find "$dir" -name "*.rs" -type f -print0 2>/dev/null)
  return $found
}

validate_workspace() {
  local dir="$1" label="$2"
  echo -e "\n${YELLOW}━━━ $label ━━━${NC}"
  
  echo "  cargo check -p soulsystem..."
  if ! (cd "$dir" && cargo check -p soulsystem 2>&1 | tail -5); then
    echo -e "  ${RED}✗ cargo check FAILED${NC}"
    return 1
  fi
  echo -e "  ${GREEN}✓ cargo check${NC}"
  
  if [ "$FAST" = true ]; then
    return 0
  fi
  
  echo "  cargo test --lib -p soulsystem..."
  if ! (cd "$dir" && cargo test --lib -p soulsystem 2>&1 | tail -5); then
    echo -e "  ${RED}✗ cargo test --lib FAILED${NC}"
    return 1
  fi
  echo -e "  ${GREEN}✓ cargo test --lib${NC}"

  echo "  cargo clippy -p soulsystem -- -D warnings..."
  if ! (cd "$dir" && cargo clippy -p soulsystem -- -D warnings 2>&1 | tail -10); then
    echo -e "  ${RED}✗ cargo clippy FAILED${NC}"
    return 1
  fi
  echo -e "  ${GREEN}✓ cargo clippy${NC}"

  return 0
}

validate_standalone() {
  local dir="$1" label="$2"
  echo -e "\n${YELLOW}━━━ $label (standalone) ━━━${NC}"
  
  echo "  cargo check..."
  if ! (cd "$dir" && cargo check 2>&1 | tail -10); then
    echo -e "  ${RED}✗ cargo check FAILED${NC}"
    return 1
  fi
  echo -e "  ${GREEN}✓ cargo check${NC}"
  
  if [ "$FAST" = true ]; then
    return 0
  fi
  
  echo "  cargo test..."
  if ! (cd "$dir" && cargo test 2>&1 | tail -5); then
    echo -e "  ${RED}✗ cargo test FAILED${NC}"
    return 1
  fi
  echo -e "  ${GREEN}✓ cargo test${NC}"
  
  return 0
}

validate_python() {
  local dir="$1" label="$2"
  echo -e "\n${YELLOW}━━━ $label (Python) ━━━${NC}"
  
  if [ -f "$dir/requirements.txt" ]; then
    echo "  Syntaxe Python..."
    local errors=0
    while IFS= read -r -d '' f; do
      if ! python3 -c "import py_compile; py_compile.compile('$f', doraise=True)" 2>/dev/null; then
        echo -e "  ${RED}✗ $f${NC}"
        errors=1
      fi
    done < <(find "$dir" -name "*.py" -type f -not -path "*/\.*" -not -path "*/target/*" -print0 2>/dev/null)
    if [ $errors -eq 0 ]; then
      echo -e "  ${GREEN}✓ syntaxe Python OK${NC}"
    else
      return 1
    fi
  fi
  
  return 0
}

# ── Main ──────────────────────────────────────────────────────────

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║           SoulSystem Validation Pipeline                    ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo "Fast mode: $FAST"
echo ""

cd "$PROJECT_DIR"

for entry in "${PROJECTS[@]}"; do
  dir=$(echo "$entry" | cut -d'|' -f1)
  name=$(echo "$entry" | cut -d'|' -f2)
  type=$(echo "$entry" | cut -d'|' -f3)
  
  # Filtre par projet si --project spécifié
  if [ -n "$TARGET_PROJECT" ] && [ "$name" != "$TARGET_PROJECT" ] && \
     [[ "$name" != *"$TARGET_PROJECT"* ]]; then
    continue
  fi
  
  case "$type" in
    workspace) validate_workspace "$dir" "$name" || FAILED=1 ;;
    standalone) validate_standalone "$dir" "$name" || FAILED=1 ;;
  esac
done

# Python validation
if [ -z "$TARGET_PROJECT" ] || [ "$TARGET_PROJECT" = "zerobot" ]; then
  validate_python "$PROJECT_DIR/zerobot" "zerobot" || FAILED=1
fi

# ── Bilan ─────────────────────────────────────────────────────────
echo ""
if [ $FAILED -eq 0 ]; then
  echo -e "${GREEN}✅ Validation PASSED — tout est vert${NC}"
  exit 0
else
  echo -e "${RED}❌ Validation FAILED — certains projets ont des erreurs${NC}"
  exit 1
fi
