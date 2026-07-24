#!/bin/bash
# clawd-supervisor.sh - Heterogeneous POSIX Supervisor
set -e

LOCKFILE="/run/user/$(id -u)/clawd-supervisor.lock"
HEALTH_URL="http://localhost:9051/health"
CPU_THRESHOLD=90
CPU_DURATION=15
TEMP_COUNTER="/tmp/clawd_cpu_counter"

# 1. Concurrency Lock
exec 9>"$LOCKFILE"
if ! flock -n 9; then
    echo "Another supervisor is already running."
    exit 1
fi

echo "Supervisor started for clawd (PID: $$)"

check_cpu() {
    # Simple CPU usage check
    CPU_USAGE=$(top -bn1 | grep "Cpu(s)" | sed "s/.*, *\([0-9.]*\)%* id.*/\1/" | awk '{print 100 - $1}')
    CPU_INT=${CPU_USAGE%.*}

    if [ "$CPU_INT" -gt "$CPU_THRESHOLD" ]; then
        VAL=$(cat "$TEMP_COUNTER" 2>/dev/null || echo 0)
        VAL=$((VAL + 1))
        echo "$VAL" > "$TEMP_COUNTER"
        if [ "$VAL" -ge "$CPU_DURATION" ]; then
            echo "CPU threshold exceeded for ${CPU_DURATION}s. Triggering rollback!"
            ./scripts/clawd-snapshot.sh rollback
            echo 0 > "$TEMP_COUNTER"
        fi
    else
        echo 0 > "$TEMP_COUNTER"
    fi
}

check_heartbeat() {
    if ! curl -sf --retry 3 --retry-delay 1 --connect-timeout 2 "$HEALTH_URL" > /dev/null; then
        echo "Heartbeat failed! Attempting restart..."
        if [ -z "$MOCK_SYSTEMD" ]; then
            systemctl --user restart clawd-container.service
        else
            echo "[MOCK] systemctl --user restart clawd-container.service"
        fi
    fi
}

# 2. Main Loop
while true; do
    check_heartbeat
    check_cpu

    # Simulate log analysis for critical errors
    # In a real scenario, we'd tail journalctl or log files
    if journalctl --user -u clawd-container.service -n 50 | grep -Ei "SIGSEGV|core dumped|oom-killer" > /dev/null; then
        echo "Critical error detected in logs! Triggering emergency rollback."
        ./scripts/clawd-snapshot.sh rollback
    fi

    sleep 1
done
