#!/bin/bash
# test-isolation.sh - Verification of Phase 1 scenarios
set -e

echo "Starting Phase 1 Verification..."

export MOCK_SYSTEMD=1

# 1. Test Lock Concurrency
echo "Testing supervisor locking..."
scripts/clawd-supervisor.sh &
PID=$!
sleep 2

if scripts/clawd-supervisor.sh; then
    echo "FAILED: Second supervisor started (lock ignored)"
    kill $PID
    exit 1
else
    echo "SUCCESS: Concurrency lock active"
fi
kill $PID || true

# 2. Test Snapshot Hybrid Logic
echo "Testing hybrid snapshot logic..."
TEST_DIR="/tmp/soulsystem_test_config"
mkdir -p "$TEST_DIR"
echo "v1" > "$TEST_DIR/data.txt"

# Mock the snapshot script variables
export BASE_DIR="/tmp/soulsystem_test"
export CONFIG_DIR="$TEST_DIR"
export SNAPSHOT_DIR="$BASE_DIR/snapshots"
mkdir -p "$SNAPSHOT_DIR"

# Run snapshot
./scripts/clawd-snapshot.sh snapshot
if [ -d "$SNAPSHOT_DIR"/snap_* ]; then
    echo "SUCCESS: Snapshot created"
else
    echo "FAILED: Snapshot not created"
    exit 1
fi

# 3. Test Rollback
echo "Testing rollback..."
echo "v2" > "$TEST_DIR/data.txt"
./scripts/clawd-snapshot.sh rollback || true # systemctl might fail in sandbox
if [ "$(cat "$TEST_DIR/data.txt")" == "v1" ]; then
    echo "SUCCESS: Rollback restored v1"
else
    echo "FAILED: Rollback did not restore v1"
    exit 1
fi

echo "Phase 1 Verification Complete."
