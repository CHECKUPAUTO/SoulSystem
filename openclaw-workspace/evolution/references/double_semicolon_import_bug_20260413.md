# Double-Semicolon Import Bug (Barrel Bypass Campaign)

**Date:** 2026-04-13  
**Severity:** P0  
**Origin:** barrel-bypassing-campaign-20260412

## Problem

The barrel-bypassing campaign replaced `index.js` barrel imports with direct module imports (`registry.js`, `approvals.js`, `runtime.js`) but introduced `";;"` (double semicolons) in 104 source files.

## Root Cause

Automated find-replace operation changed import paths from `../channels/plugins/index.js` to `../channels/plugins/registry.js` but the replacement pattern resulted in `";;"` instead of `";"`.

## Affected Files

- ~97 files importing from `channels/plugins/registry.js`
- 4 files importing from `channels/plugins/approvals.js`  
- 2+ files importing from `plugins/runtime/runtime.js`

## Fix

```bash
cd /mnt/nvme_secondary/ai_projects/openclaw
find src/ -name "*.ts" -not -name "*.test.ts" -exec sed -i 's/from "\([^"]*\)";;/from "\1";/g' {} +
```

## Prevention

- Enable `no-extra-semi` eslint rule at error level
- Add pre-commit hook to detect `";;"` in TypeScript files

## Status

- **Confirmed by 2 independent night cycles** (00:15 and 00:19 reports)
- **104 files affected** — consistent count across reports
- **No lint rule currently catches this** — `no-extra-semi` not configured at error level
- **Action required:** Apply one-liner fix before next release

## Related

- `barrel_bypassing_guide.md` — origin of the barrel bypass campaign
- `lint_rule_plugin_import.md` — related lint rule proposals