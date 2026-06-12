# Plugin Peer Resolution Fragility

**Priority:** P1 (Architectural)
**Source:** Night Cycle 2026-04-13 01:47 (commits `ad7f605`, `d77360c`)
**Status:** Reference documentation
**Created:** 2026-04-13

## Problem

Two related fixes in one release cycle for the same subsystem (plugin bundling):
1. `ad7f605`: Peer resolution now tolerates mismatched bundled deps
2. `d77360c`: Missing native runtime deps restored

**Pattern:** Plugin dependency resolution is fragile — two hotfixes in one cycle suggests systemic issues.

## Recommendations

1. **Add dependency matrix validation to CI**:
   ```yaml
   - name: Validate plugin peer deps
     run: npx openclaw-plugin-check --validate-peers --matrix
   ```

2. **Plugin resolution integration test suite**:
   ```typescript
   describe('Plugin Resolution', () => {
     it('should tolerate missing bundled peers', async () => {});
     it('should restore native runtime deps', async () => {});
     it('should handle mixed bundled/external deps', async () => {});
   });
   ```

3. **Document peer dependency contract** for plugin authors

4. **Add release baseline stability check** — commits `4503a43` and `b2f94d9` refresh generated release baselines, suggesting config drift

## Cross-References

- `bundled_plugin_resolution.md` — Existing reference on plugin bundling
- `plugin_import_best_practices_2026-04-11.md` — Import patterns
- `barrel_bypassing_guide.md` — Barrel elimination campaign