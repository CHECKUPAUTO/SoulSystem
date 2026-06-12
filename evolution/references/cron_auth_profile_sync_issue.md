# Cron Auth Profile Sync Issue

> From night_cycle_20260413_1300.md

## Issue

Multiple isolated cron agents are experiencing `FailoverError` due to missing OpenAI API keys in their isolated session directories.

**Affected crons**:
- `Healthcheck Auto`
- `clawd-state-persistence`
- `OpenEvolve Auto-Apply`
- `Morning Briefing`

**Root cause**: `auth-profiles.json` is not synced to isolated agent directories. When cron spawns an isolated session, it doesn't have access to the main session's auth profile, causing API key lookups to fail.

## Proposed Fix

Copy `auth-profiles.json` to fix failing healthchecks and persistence crons in isolated agent directories.

## Status

⚠️ **Requires manual review** — syncing auth profiles to isolated directories is security-sensitive (credential propagation). Should be reviewed before implementation.

## Possible Approaches

1. **Symlink**: Create a symlink from isolated agent dirs to the main auth-profiles.json
2. **Copy on spawn**: Add a pre-spawn hook that copies auth profiles
3. **Gateway-level**: Configure the gateway to inject auth profiles into isolated sessions
4. **Environment variables**: Use env vars instead of file-based auth for isolated sessions

## Last Updated

2026-04-13T17:42:00+02:00 — Auto-apply cycle (documented as tracking reference only)