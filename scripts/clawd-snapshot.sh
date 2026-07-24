#!/bin/bash
# clawd-snapshot.sh - Hybrid Snapshot & Rollback
set -e

BASE_DIR="${BASE_DIR:-/opt/soulsystem}"
CONFIG_DIR="${CONFIG_DIR:-$BASE_DIR/config}"
SNAPSHOT_DIR="${SNAPSHOT_DIR:-$BASE_DIR/snapshots}"
MAX_SNAPSHOTS=3

mkdir -p "$SNAPSHOT_DIR"

FS_TYPE=$(stat -f --format=%T "$CONFIG_DIR")

do_snapshot() {
    TS=$(date +%Y%m%d%H%M%S)
    TARGET="$SNAPSHOT_DIR/snap_$TS"

    if [ "$FS_TYPE" == "btrfs" ]; then
        echo "BTRFS detected. Creating subvolume snapshot..."
        btrfs subvolume snapshot -r "$CONFIG_DIR" "$TARGET"
    else
        echo "Non-BTRFS ($FS_TYPE) detected. Creating rsync hardlink copy..."
        LATEST=$(ls -td "$SNAPSHOT_DIR"/snap_* 2>/dev/null | head -n 1 || true)
        if [ -n "$LATEST" ]; then
            rsync -a --delete --link-dest="$LATEST" "$CONFIG_DIR/" "$TARGET/"
        else
            rsync -a "$CONFIG_DIR/" "$TARGET/"
        fi
    fi

    # Rotation: Keep only 3 most recent
    ls -td "$SNAPSHOT_DIR"/snap_* | tail -n +$((MAX_SNAPSHOTS + 1)) | xargs -r rm -rf
}

do_rollback() {
    LATEST=$(ls -td "$SNAPSHOT_DIR"/snap_* 2>/dev/null | head -n 1 || true)
    if [ -z "$LATEST" ]; then
        echo "No snapshots found for rollback!"
        exit 1
    fi

    echo "Rolling back to $LATEST..."
    if [ "$FS_TYPE" == "btrfs" ]; then
        btrfs subvolume delete "$CONFIG_DIR"
        btrfs subvolume snapshot "$LATEST" "$CONFIG_DIR"
    else
        rm -rf "$CONFIG_DIR"/*
        cp -al "$LATEST"/* "$CONFIG_DIR/"
    fi
    if [ -z "$MOCK_SYSTEMD" ]; then
        systemctl --user restart clawd-container.service
    else
        echo "[MOCK] systemctl --user restart clawd-container.service"
    fi
}

case "$1" in
    snapshot) do_snapshot ;;
    rollback) do_rollback ;;
    *) echo "Usage: $0 {snapshot|rollback}" ;;
esac
