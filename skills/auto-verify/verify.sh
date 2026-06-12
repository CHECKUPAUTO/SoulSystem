#!/bin/bash
# Auto-Verify Pipeline — exécuté après chaque edit/write/exec touchant du code

set -euo pipefail

FILE="$1"
EXIT_CODE=0
RETRIES=0
MAX_RETRIES=3

SCRIPT_NAME="$(basename "$0")"
BASENAME="$(basename "$FILE")"

run_check() {
  local file="$1"
  local has_error=0

  echo "🔍 [auto-verify] $file"

  # 1. Syntax Check
  case "$file" in
    *.py)
      if ! python3 -m py_compile "$file" 2>/dev/null; then
        echo "❌ Syntaxe Python invalide"
        has_error=1
      else
        echo "✅ Syntaxe Python OK"
      fi
      ;;
    *.js|*.mjs|*.cjs)
      if ! node --check "$file" 2>/dev/null; then
        echo "❌ Syntaxe JS invalide"
        has_error=1
      else
        echo "✅ Syntaxe JS OK"
      fi
      ;;
    *.ts|*.tsx)
      if command -v tsc >/dev/null 2>&1; then
        if ! tsc --noEmit --skipLibCheck "$file" 2>/dev/null; then
          echo "❌ TypeScript invalide"
          has_error=1
        else
          echo "✅ TypeScript OK"
        fi
      else
        echo "⚠️ tsc non disponible, skip TypeScript check"
      fi
      ;;
    *.rs)
      if command -v rustc >/dev/null 2>&1; then
        if ! rustc --edition 2021 --crate-type lib -Z parse-only "$file" 2>/dev/null; then
          echo "❌ Syntaxe Rust invalide"
          has_error=1
        else
          echo "✅ Syntaxe Rust OK"
        fi
      else
        echo "⚠️ rustc non disponible, skip Rust check"
      fi
      ;;
    *.sh)
      if ! bash -n "$file" 2>/dev/null; then
        echo "❌ Syntaxe Bash invalide"
        has_error=1
      else
        echo "✅ Syntaxe Bash OK"
      fi
      ;;
    *.json)
      if ! python3 -m json.tool "$file" > /dev/null 2>&1; then
        echo "❌ JSON invalide"
        has_error=1
      else
        echo "✅ JSON OK"
      fi
      ;;
    *)
      echo "ℹ️ Pas de check syntaxe pour cette extension"
      ;;
  esac

  # 2. Stub Check — skip documentation et self
  case "$file" in
    *.md|*.txt|*.rst)
      echo "ℹ️ Skip stub check pour documentation"
      ;;
    *)
      if [ "$BASENAME" = "$SCRIPT_NAME" ]; then
        echo "ℹ️ Skip stub check sur verify.sh (self-check)"
      elif grep -qE 'TODO|FIXME|pass;?$|unimplemented|\bXXX\b' "$file" 2>/dev/null; then
        echo "⚠️ STUB DETECTED"
        has_error=1
      fi
      ;;
  esac

  # 3. Secret Check — skip documentation et self
  case "$file" in
    *.md|*.txt|*.rst)
      echo "ℹ️ Skip secret check pour documentation"
      ;;
    *)
      if [ "$BASENAME" = "$SCRIPT_NAME" ]; then
        echo "ℹ️ Skip secret check sur verify.sh (self-check)"
      elif grep -qiE 'api[_-]?key|password|secret|ghp_|sk-|token[_-]?id|private[_-]?key' "$file" 2>/dev/null; then
        echo "🚨 SECRET POTENTIEL DETECTE"
        has_error=1
      fi
      ;;
  esac

  # 4. Empty / Hallucination Check
  if [ ! -s "$file" ]; then
    echo "🚨 FICHIER VIDE"
    has_error=1
  fi

  local lines
  lines=$(wc -l < "$file" | tr -d ' ')
  if [ "$lines" -gt 0 ]; then
    local uniq_lines
    uniq_lines=$(sort -u "$file" | wc -l | tr -d ' ')
    if [ "$uniq_lines" -lt $((lines / 2)) ] && [ "$lines" -gt 10 ]; then
      echo "⚠️ TROP DE LIGNES DUPLIQUEES (hallucination possible)"
      has_error=1
    fi
  fi

  return $has_error
}

# Auto-retry loop
while [ $RETRIES -lt $MAX_RETRIES ]; do
  if run_check "$FILE"; then
    echo "✅ [auto-verify] Tous les checks passent"
    exit 0
  fi

  RETRIES=$((RETRIES + 1))
  echo "🔄 [auto-verify] Retry $RETRIES/$MAX_RETRIES"
done

echo "❌ [auto-verify] Échec après $MAX_RETRIES tentatives"
exit 1
