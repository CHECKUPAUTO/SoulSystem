# Dependency Denylist: Transitive Coverage Proposal

**Created:** 2026-04-13 (Night Cycle 00:30)
**Source:** OpenEvolve Night Cycle Report 2026-04-13 00:30
**Status:** Proposal
**Priority:** P1

## Context

The Codex harness introduces a pluggable agent system that could theoretically install arbitrary npm packages. Security fixes include:
- `9f97ad857a` — Pin axios to 1.15.0 + add dependency denylist for plugin installs
- `4ad4ee1962` — Expand host env security policy denylist
- `2bd56b8c38` — Keep legacy ssrf alias raw-config only

## Current Gap

The dependency denylist currently covers top-level packages only. A malicious package could be installed as a transitive dependency of an allowed package.

## Proposal: Transitive Dependency Scanning

### Pre-Install Gate

```typescript
async function validatePluginDependencies(packageSpec: string): Promise<ValidationResult> {
  // 1. Resolve dependency tree without installing
  const tree = await resolveDependencyTree(packageSpec);

  // 2. Check denylist at all levels
  for (const dep of tree.allDependencies) {
    if (DENYLIST.has(dep.name)) {
      return { valid: false, reason: `Denied package ${dep.name} found as transitive dependency` };
    }
  }

  // 3. Run npm audit
  const audit = await runAudit(packageSpec);
  if (audit.vulnerabilities.high > 0 || audit.vulnerabilities.critical > 0) {
    return { valid: false, reason: `High/critical vulnerabilities found: ${audit.summary}` };
  }

  return { valid: true };
}
```

### Configuration

```json
{
  "denylist": {
    "direct": ["axios@<1.15.0", "request", "node-uuid"],
    "transitive": true,
    "auditLevel": "high",
    "allowOverrides": {
      "axios": ">=1.15.0"
    }
  }
}
```

## References

- Security audit patterns: `evolution/references/security_audit_patterns.md`
- Security fixes: `9f97ad857a`, `4ad4ee1962`, `2bd56b8c38`