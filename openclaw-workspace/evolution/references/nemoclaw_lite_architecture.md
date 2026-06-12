# nemoclaw-lite Architecture

**Based on OpenEvolve Night Cycle Report 2026-04-11**  
**Source Commit:** 401f2e999e  
**Generated:** Auto-applied 2026-04-11 23:08 UTC

## Overview

Lightweight variant of nemoclaw for resource-constrained environments.

## Architecture

```
nemoclaw-lite/
├── core/
│   ├── state-manager.ts      # Minimal state tracking
│   └── event-bus.ts          # Lightweight pub/sub
├── plugins/
│   └── essential/            # Core plugin subset only
└── gateway/
    └── minimal-gateway.ts    # Stripped-down gateway
```

## Differences from Full nemoclaw

| Feature | nemoclaw | nemoclaw-lite |
|---------|----------|---------------|
| Plugin System | Full | Essential only |
| State Manager | Redux-like | Simple observable |
| Gateway | Full protocol | Minimal HTTP |
| Memory Footprint | ~150MB | ~50MB |
| Startup Time | ~5s | ~1s |

## Use Cases

- Edge devices with limited RAM
- Quick-start development environments
- CI/CD pipeline integration
- Resource-constrained VPS deployments

## Configuration

```typescript
// nemoclaw-lite.config.ts
export default {
  mode: 'lite',
  plugins: ['essential/core', 'essential/logging'],
  gateway: {
    protocol: 'http',
    port: 8080
  },
  state: {
    persistence: false,
    maxHistory: 10
  }
};
```

## Migration Path

Full nemoclaw → nemoclaw-lite:
1. Audit plugin dependencies
2. Replace full gateway with minimal
3. Migrate state management
4. Test core functionality

nemoclaw-lite → nemoclaw:
1. Install full gateway
2. Enable full plugin system
3. Migrate state to Redux pattern
4. Validate all integrations

---
*Auto-generated from Night Cycle analysis*
*Last Updated: 2026-04-11*
