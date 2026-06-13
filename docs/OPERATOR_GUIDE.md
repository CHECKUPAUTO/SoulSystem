# SoulSystem Operator Guide

## Starting the system

```bash
./target/release/soulsystem
```

## Monitoring

- **Web dashboard**: `http://localhost:9090`
- **TUI**: `cargo run -p soul-top`
- **Logs**: `/var/log/soulsystem/`
- **Prometheus metrics**: `http://localhost:9100/metrics`

## Configuration

Configuration is done through `soulsystem.toml` and environment variables (`SOULSYSTEM_*`).

## Health Check

The system exposes health endpoints:
- `http://localhost:9023/health` — overall system status
- `http://localhost:9020/health` — orchestrator status

## Backup

Backup the sled database and SQLite stores before upgrades:
```bash
cp -r /var/lib/soulsystem /var/lib/soulsystem.backup
```