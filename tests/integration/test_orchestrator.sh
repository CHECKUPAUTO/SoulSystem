#!/bin/bash
# test_orchestrator.sh — Test d'intégration end-to-end pour SAI Orchestrator

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
ORCHESTRATOR="$ROOT_DIR/skills/sai-orchestrator/orchestrate.sh"
VERIFY="$ROOT_DIR/skills/auto-verify/verify.sh"

PASSED=0
FAILED=0

run_test() {
  local name="$1"
  local cmd="$2"
  local expected="${3:-0}"

  echo "🧪 Test: $name"
  if eval "$cmd" >/dev/null 2>&1; then
    actual=0
  else
    actual=1
  fi

  if [ "$actual" -eq "$expected" ]; then
    echo "   ✅ PASS"
    PASSED=$((PASSED + 1))
  else
    echo "   ❌ FAIL (expected exit $expected, got $actual)"
    FAILED=$((FAILED + 1))
  fi
}

# Test 1: détection mode projet (fichiers > 3)
run_test "detect mode project via files" \
  "bash $ORCHESTRATOR --files 4 --cmds 0 2>&1 | grep -q 'project'"

# Test 2: détection mode interactif (fichiers ≤ 3)
run_test "detect mode interactive" \
  "bash $ORCHESTRATOR --files 2 --cmds 1 2>&1 | grep -q 'interactive'"

# Test 3: détection via mots-clés
run_test "detect mode project via keyword" \
  "bash $ORCHESTRATOR --files 1 --cmds 0 --mode project 2>&1 | grep -q 'project'"

# Test 4: verify passe sur fichier valide
run_test "verify valid shell script" \
  "bash $VERIFY $ORCHESTRATOR"

# Test 5: verify échoue sur fichier avec stub
TMP_STUB=$(mktemp /tmp/stub_test.XXXXXX.sh)
echo '#!/bin/bash
echo 'STUB_PLACEHOLDER fix this'' > "$TMP_STUB"
run_test "verify catches stub" \
  "bash $VERIFY $TMP_STUB" 1
rm -f "$TMP_STUB"

# Test 6: verify skip documentation
TMP_MD=$(mktemp /tmp/doc_test.XXXXXX.md)
echo '# Title
| pass | ok |' > "$TMP_MD"
run_test "verify skips markdown stubs" \
  "bash $VERIFY $TMP_MD"
rm -f "$TMP_MD"

# Test 7: verify skip self-check
run_test "verify skips self-check" \
  "bash $VERIFY $VERIFY"

# Test 8: log.sh fonctions chargées
run_test "log.sh loads without error" \
  "bash -c 'source $ROOT_DIR/skills/sai-orchestrator/lib/log.sh'"

# Summary
echo ""
echo "═══════════════════════════════════════"
echo "  Résultats : $PASSED pass, $FAILED fail"
echo "═══════════════════════════════════════"

if [ "$FAILED" -gt 0 ]; then
  exit 1
fi
