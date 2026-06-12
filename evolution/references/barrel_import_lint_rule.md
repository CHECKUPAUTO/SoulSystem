# Barrel Import Lint Rule Proposal

**Created:** 2026-04-13 (Night Cycle 00:34)
**Source:** OpenEvolve Night Cycle Report 2026-04-13 00:34
**Status:** Proposal
**Priority:** P1

## Context

The barrel-avoidance campaign (40+ commits over 3 days) has systematically removed barrel (re-export `index.ts`) files from hot paths. However, there's no automated prevention of regressions — new barrel imports could be reintroduced accidentally.

## Proposal: AST-Based Barrel Import Detection

### ESLint Custom Rule: `no-barrel-imports`

```javascript
// .eslintrc.js — custom rule
{
  "rules": {
    "custom/no-barrel-imports": ["error", {
      "barrelPatterns": ["**/index.ts", "**/index.js"],
      "hotPaths": [
        "src/channels/**/registry.ts",
        "src/plugins/runtime/**",
        "src/gateway/session/**"
      ],
      "allowIn": ["src/**/test/**", "src/**/__tests__/**"]
    }]
  }
}
```

### Detection Logic

1. Resolve import path to file system
2. Check if resolved path is a barrel file (index.ts/index.js with re-exports)
3. If in a hot-path module, flag as error
4. If in test module, allow

### CI Integration

```yaml
# .github/workflows/barrel-check.yml
- name: Barrel Import Check
  run: npx eslint --rule 'custom/no-barrel-imports:error' 'src/**/*.ts'
```

### Perf Budget Alternative

Create `perf-budget.json` tracking import depth per module:

```json
{
  "modules": {
    "src/gateway/session/store.ts": { "maxImportDepth": 3 },
    "src/channels/plugins/registry.ts": { "maxImportDepth": 4 }
  },
  "defaults": { "maxImportDepth": 5 }
}
```

Run in CI: `node scripts/check-import-depths.js --budget perf-budget.json`

## References

- Barrel bypassing guide: `evolution/references/barrel_bypassing_guide.md`
- Explicit seams pattern: `evolution/references/explicit_seams_pattern.md`
- 40+ barrel-avoidance commits (April 10-12, 2026)