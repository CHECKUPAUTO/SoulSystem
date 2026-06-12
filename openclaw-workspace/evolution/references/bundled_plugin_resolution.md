# Bundled Plugin Resolution Tolerance

**Source:** Night Cycle 2026-04-13 01:47 (commit ad7f605, d77360c)
**Status:** Reference documentation
**Priority:** P2

## Pattern: Graceful Peer Resolution for Bundled Plugins

When plugins are bundled (shipped with the gateway), their peer dependencies should be resolved from the gateway's own dependencies rather than requiring separate installation. The recent fixes (`ad7f605`, `d77360c`) address two issues:

### Problem 1: Peer Resolution Failure
Bundled plugins' peer dependencies were failing resolution because the plugin system expected each plugin to manage its own dependency tree, even when bundled.

**Fix:** Tolerate bundled peer resolution — when a plugin is bundled, resolve its peers from the gateway's dependency tree.

### Problem 2: Missing Native Runtime Dependencies
Native modules (node-gyp compiled) were not being restored correctly for bundled plugins after installation or update.

**Fix:** Restore missing native runtime dependencies for bundled plugins during startup.

### General Pattern

```typescript
// When resolving dependencies for bundled plugins:
if (plugin.isBundled) {
  // Resolve peers from gateway's own dependencies
  const resolved = gatewayDependencies.get(peerName) ?? tryResolve(peerName, { paths: [gatewayRoot] });
  if (!resolved) {
    // Tolerate: log warning but don't block plugin load
    logger.warn(`Bundled peer ${peerName} not resolved for ${plugin.name}`);
  }
} else {
  // Standard resolution for user-installed plugins
  const resolved = resolveFromPluginDir(peerName);
}
```

### Test Pattern: Lazy-Loading Fixtures

Related pattern from `f619368` (test: lazy-load auth and gateway fixtures) and `c473b17` (test: defer bundled plugin contract loads):

Tests should lazy-load heavy fixtures (auth, gateway, plugin contracts) rather than importing them at module level. This aligns with the barrel-avoidance campaign — defer what you don't need immediately.

```typescript
// Before: eager import
import { createAuthFixture } from '../fixtures/auth.js';

// After: lazy factory
let _auth: AuthFixture;
const auth = () => _auth ??= createAuthFixture();
```

### Cross-References

- Barrel bypassing guide (`barrel_bypassing_guide.md`)
- Performance optimization patterns (`performance_optimization_patterns.md`)
- CI reliability test purity (`ci_reliability_test_purity.md`)