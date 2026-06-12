# Plugin Version Matrix Testing

**Source:** OpenEvolve Night Cycle Report 2026-04-12 03:15  
**Priority:** P2  
**Related Commit:** 0e3f965 (fix(plugins): preserve bundled host compatibility floor)

---

## Background

Commit 0e3f965 addressed plugin compatibility issues by adding bundled host compatibility floor checks. This indicates version compatibility is becoming critical as the plugin ecosystem grows.

---

## Version Matrix Testing Strategy

### 1. Compatibility Requirements

```typescript
// plugins/compatibility.ts
export interface CompatibilityMatrix {
  hostVersion: string;
  pluginVersions: PluginVersionEntry[];
}

export interface PluginVersionEntry {
  pluginName: string;
  pluginVersion: string;
  minHostVersion: string;
  maxHostVersion?: string;
  status: 'compatible' | 'deprecated' | 'incompatible';
  notes?: string;
}
```

### 2. CI Matrix Configuration

```yaml
# .github/workflows/plugin-matrix.yml
name: Plugin Version Matrix

on:
  push:
    branches: [main]
  pull_request:
    paths:
      - 'src/plugins/**'
      - 'extensions/**'

jobs:
  compatibility-matrix:
    runs-on: ubuntu-latest
    strategy:
      fail-fast: false
      matrix:
        host-version:
          - '2026.4.11'
          - '2026.4.10'
          - '2026.4.9'
          - '2026.3.x'
        plugin:
          - 'active-memory'
          - 'canvas-tools'
          - 'cron-scheduler'
          - 'mcp-client'

    steps:
      - uses: actions/checkout@v4

      - name: Setup OpenClaw ${{ matrix.host-version }}
        uses: ./.github/actions/setup-openclaw
        with:
          version: ${{ matrix.host-version }}

      - name: Install Plugin
        run: |
          openclaw plugin install ${{ matrix.plugin }} --version latest

      - name: Run Compatibility Tests
        run: |
          npm run test:plugin:compatibility -- \
            --plugin=${{ matrix.plugin }} \
            --host-version=${{ matrix.host-version }}

      - name: Report Compatibility
        if: always()
        run: |
          node scripts/report-compatibility.js \
            --plugin=${{ matrix.plugin }} \
            --host-version=${{ matrix.host-version }} \
            --result=${{ job.status }}
```

### 3. Compatibility Test Suite

```typescript
// test/plugins/compatibility.test.ts
import { describe, it, expect } from 'vitest';
import { checkCompatibility, loadPlugin } from '../../src/plugins';

describe('Plugin Version Compatibility', () => {
  const hostVersion = process.env.OPENCLAW_VERSION || '2026.4.11';

  describe('Active Memory Extension', () => {
    it(`should be compatible with host ${hostVersion}`, async () => {
      const result = await checkCompatibility({
        plugin: 'active-memory',
        hostVersion,
      });

      expect(result.compatible).toBe(true);
      expect(result.blockers).toHaveLength(0);
    });

    it('should respect minimum host version', async () => {
      const result = await checkCompatibility({
        plugin: 'active-memory',
        hostVersion: '2026.3.0', // Too old
      });

      expect(result.compatible).toBe(false);
      expect(result.blockers).toContain('MIN_HOST_VERSION');
    });

    it('should warn about deprecated plugin versions', async () => {
      const result = await checkCompatibility({
        plugin: 'active-memory',
        pluginVersion: '1.0.0', // Old version
        hostVersion,
      });

      expect(result.compatible).toBe(true);
      expect(result.warnings).toContain('DEPRECATED_VERSION');
    });
  });

  describe('All Bundled Plugins', () => {
    const bundledPlugins = [
      'active-memory',
      'canvas-tools',
      'cron-scheduler',
      'mcp-client',
      'notification-bridge',
    ];

    it.each(bundledPlugins)(
      'should validate %s compatibility',
      async (pluginName) => {
        const result = await checkCompatibility({
          plugin: pluginName,
          hostVersion,
        });

        expect(result.compatible).toBe(true);
        expect(result.errors).toHaveLength(0);
      }
    );
  });
});
```

### 4. Manifest Validation

```typescript
// src/plugins/validate-manifest.ts
export function validatePluginManifest(
  manifest: PluginManifest
): ValidationResult {
  const errors: string[] = [];
  const warnings: string[] = [];

  // Required fields
  if (!manifest.compatibility?.minHostVersion) {
    errors.push('Missing minHostVersion in compatibility');
  }

  // Version format validation
  if (manifest.compatibility?.minHostVersion) {
    if (!isValidSemver(manifest.compatibility.minHostVersion)) {
      errors.push('minHostVersion must be valid semver');
    }
  }

  // Max version constraint
  if (manifest.compatibility?.maxHostVersion) {
    if (!isValidSemver(manifest.compatibility.maxHostVersion)) {
      errors.push('maxHostVersion must be valid semver');
    }

    if (manifest.compatibility.minHostVersion &&
        semverGte(manifest.compatibility.minHostVersion,
                  manifest.compatibility.maxHostVersion)) {
      errors.push('minHostVersion must be less than maxHostVersion');
    }
  }

  // Deprecation warning
  if (manifest.deprecated) {
    warnings.push(`Plugin ${manifest.name} is deprecated: ${manifest.deprecated.reason}`);
  }

  return { valid: errors.length === 0, errors, warnings };
}
```

### 5. Compatibility Report Generation

```typescript
// scripts/generate-compatibility-report.ts
import { readdir } from 'fs/promises';
import { checkCompatibility } from '../src/plugins';

async function generateReport() {
  const hostVersions = ['2026.4.11', '2026.4.10', '2026.4.9', '2026.3.x'];
  const plugins = await readdir('./extensions');

  const results: CompatibilityResult[] = [];

  for (const hostVersion of hostVersions) {
    for (const plugin of plugins) {
      const result = await checkCompatibility({ plugin, hostVersion });
      results.push({
        hostVersion,
        plugin,
        compatible: result.compatible,
        blockers: result.blockers,
        warnings: result.warnings,
      });
    }
  }

  // Generate markdown table
  const table = generateMarkdownTable(results);
  await writeFile('./COMPATIBILITY.md', table);

  // Generate JSON for programmatic access
  await writeFile('./compatibility.json', JSON.stringify(results, null, 2));
}

function generateMarkdownTable(results: CompatibilityResult[]): string {
  // Generate compatibility matrix table
  // ...
}

generateReport().catch(console.error);
```

---

## Compatibility Matrix Output

```markdown
# OpenClaw Plugin Compatibility Matrix

## Legend
- ✅ Fully compatible
- ⚠️ Compatible with warnings
- ❌ Incompatible
- 🔄 Deprecated

## Results

| Plugin | 2026.4.11 | 2026.4.10 | 2026.4.9 | 2026.3.x |
|--------|-----------|-----------|----------|----------|
| active-memory | ✅ | ✅ | ⚠️ | ❌ |
| canvas-tools | ✅ | ✅ | ✅ | ✅ |
| cron-scheduler | ✅ | ✅ | ✅ | ⚠️ |
| mcp-client | ✅ | ✅ | ❌ | ❌ |

## Notes

### active-memory
- Requires host 2026.4.9+ for context preservation features
- Deprecated on 2026.3.x

### cron-scheduler
- Basic functionality works on 2026.3.x
- Advanced features require 2026.4.0+
```

---

## References

- Source Report: `night_cycle_20260412_0315.md`
- Related Commit: 0e3f965
- Related Pattern: `plugin_avoidance_pattern_2026-04-11.md`
