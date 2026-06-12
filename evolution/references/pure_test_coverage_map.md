# Pure Test Coverage Map

*Created: 2026-04-13 (Night Cycle)*
*Source: night_cycle_20260413_0001.md*

## Purpose

Track which OpenClaw modules have been migrated to pure test coverage, identify gaps, and prevent duplicate testing.

## Migration Status (as of 2026-04-12)

### ✅ Migrated to Pure Coverage
| Module | Commit(s) | Notes |
|--------|-----------|-------|
| Plugin list formatting | `2681bbd` | Moved from integration |
| Plugin uninstall/update | `8fb48226` | Owner-based pure tests |
| Node pairing authz | `e2477ff` | Pure coverage |
| Session timeout | `367043d` | Folded into pure |
| Directive status/model override | `7e66a8f` | Narrowed |
| Queue/group parsing | `10dcd57` | Pure extraction |
| Command normalization | `2cfd145` | Split from registry |
| Queue settings | `66a0814` | Direct import |
| Spawn/doctor | Recent | Pure coverage |
| Tools invoke cron regression | `6883273` | Hardened |

### ⚠️ Partially Migrated
| Module | Status | Risk |
|--------|--------|------|
| Feishu comment handling | New tests, still god module | High — 761 lines in monitor.comment.ts |
| Codex OAuth | Scope fix tested (287 lines) | Medium — lifecycle coverage gap |
| Video generation | Types well-tested, runtime partial | Medium — new provider expansion |

### 🔴 Not Yet Migrated
| Module | Risk Level | Notes |
|--------|-----------|-------|
| iMessage monitor | High | Startup retry reliability (3 fix commits) |
| Channel lifecycle (WhatsApp) | Medium | Reconnect gap handling |
| Talk Mode (macOS) | Medium | First-execution permission flow |

## Weekly Metrics

| Week | Pure Test Commits | Integration → Pure | Coverage Ratio |
|------|-------------------|-------------------|----------------|
| 2026-W14 | 12 | 10 modules | ~65% |
| 2026-W15 (projected) | 15+ | Ongoing | Target: 75% |

## Codemod Opportunity

The test ownership migration pattern is repetitive enough for automation:

```python
# evolution/scripts/test_ownership_migrate.py
# Identifies test cases testing module X but living in file Y
# Generates move patches following the ownership pattern
# Usage: python test_ownership_migrate.py --dry-run
```

## Cross-References
- `barrel_bypassing_guide.md` — Why pure tests matter
- `narrow_surface_pattern.md` — Module boundary patterns
- `simplification_wave_tracker.md` — Broader simplification context