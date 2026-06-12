# macOS Home Directory Fallback Pattern

**Date:** 2026-04-13  
**Source:** Night Cycle Report (00:52)  
**Status:** Proposal  
**Priority:** P0 (addresses crash bug #65582)  
**Bug Reference:** Issue #65582 — ENOENT mkdir `/home/node` on macOS (no Docker)  

## Problem

On macOS without Docker, the application attempts to create `/home/node` which doesn't exist and isn't writable. This causes an ENOENT crash on startup for native macOS users.

## Pattern: Preflight Directory Validation

```typescript
import { homedir } from 'os';
import { mkdirSync, accessSync, constants } from 'fs';

function ensureDataDir(configuredPath?: string): string {
  const dir = configuredPath 
    ?? process.env.HOME 
    ?? homedir();
  
  try {
    mkdirSync(dir, { recursive: true });
    accessSync(dir, constants.W_OK);
    return dir;
  } catch (e) {
    // Fallback to system temp if primary path fails
    const fallback = path.join(os.tmpdir(), 'openclaw');
    mkdirSync(fallback, { recursive: true });
    logger.warn(`Primary data dir ${dir} unavailable, using ${fallback}`);
    return fallback;
  }
}
```

## Guidelines

1. **Never hardcode `/home/node`** — always resolve from `HOME`, `homedir()`, or config
2. **Startup preflight check** — validate directory writability before use
3. **Graceful fallback** — use `os.tmpdir()` if primary path fails
4. **Log the fallback** — operators need visibility into path resolution

## Upstream Tracking

- Issue #65582: ENOENT mkdir `/home/node` on macOS without Docker