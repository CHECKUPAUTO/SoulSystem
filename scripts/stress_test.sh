#!/usr/bin/env bash
# stress_test.sh — Test d'endurance SoulSystem
# Execute le test de charge en boucle pendant une duree donnee.
# Usage: ./stress_test.sh [duree_en_minutes]

set -euo pipefail

DURATION_MIN="${1:-5}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

echo "=== SoulSystem Stress Test ==="
echo "Duree: ${DURATION_MIN} minute(s)"
echo "Project: ${PROJECT_DIR}"
echo ""

cd "$PROJECT_DIR"

END_TIME=$(($(date +%s) + DURATION_MIN * 60))
ITERATION=0
PASSED=0
FAILED=0

while [ $(date +%s) -lt $END_TIME ]; do
    ITERATION=$((ITERATION + 1))
    echo "[$(date '+%H:%M:%S')] Iteration ${ITERATION}..."

    if cargo test --test load_test -- --nocapture 2>&1; then
        PASSED=$((PASSED + 1))
        echo "  -> PASSED"
    else
        FAILED=$((FAILED + 1))
        echo "  -> FAILED"
    fi

    # Pause de 2 secondes entre iterations
    sleep 2
done

echo ""
echo "=== Results ==="
echo "Total iterations: ${ITERATION}"
echo "Passed: ${PASSED}"
echo "Failed: ${FAILED}"

if [ $FAILED -gt 0 ]; then
    echo "STRESS TEST FAILED"
    exit 1
else
    echo "STRESS TEST PASSED"
    exit 0
fi
