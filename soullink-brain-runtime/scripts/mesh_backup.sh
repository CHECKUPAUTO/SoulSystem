#!/bin/bash
# SoulLink Mesh Backup — Sauvegarde incrémentale du mesh
set -euo pipefail

BACKUP_DIR="/mnt/nvme/soullink_brain/backups"
MESH_DIR="/mnt/nvme/soullink_brain"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RETENTION_DAYS=7

mkdir -p "$BACKUP_DIR"

# Vérifier si le mesh est actif
if ! ss -tlnp | grep -q ':9020'; then
  echo "[$(date -Iseconds)] ⚠️ Orchestrator non actif — backup skipped"
  exit 0
fi

# Backup des métriques
curl -s http://localhost:9020/api/mesh/status > "$BACKUP_DIR/mesh_status_${TIMESTAMP}.json" 2>/dev/null || true

# Backup de la mémoire
curl -s http://localhost:5030/api/stats > "$BACKUP_DIR/memory_stats_${TIMESTAMP}.json" 2>/dev/null || true

# Backup des configs
rsync -a /etc/soullink/ "$BACKUP_DIR/config_${TIMESTAMP}/" 2>/dev/null || cp -r /etc/soullink "$BACKUP_DIR/config_${TIMESTAMP}/" 2>/dev/null || true

# Rotation
find "$BACKUP_DIR" -name "*.json" -mtime +$RETENTION_DAYS -delete 2>/dev/null || true
find "$BACKUP_DIR" -maxdepth 1 -type d -name "config_*" -mtime +$RETENTION_DAYS -exec rm -rf {} + 2>/dev/null || true

echo "[$(date -Iseconds)] ✅ Backup mesh terminé: ${TIMESTAMP}"
