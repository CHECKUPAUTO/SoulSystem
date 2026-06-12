# MS Teams Enterprise Auth Consolidation

**Priority:** P2 (Medium)
**Source:** Night Cycle 2026-04-13 04:05 (commits 53615, 51646, 57530, 64645, 64641, 64646, 64652)
**Status:** Reference documentation
**Applies to:** MS Teams channel plugin, enterprise auth

---

## Problem

Multiple MS Teams auth-related fixes in a short timeframe suggest fragmented auth handling across different credential types and auth flows:

1. **Federated credentials** — certificate + managed identity support added piecemeal
2. **SingleTenant JWT validation** — issuer validation fix for `sts.windows.net`
3. **Group management** — delegated auth for add/remove participant actions
4. **Reaction support** — pagination helper with delegated auth
5. **Media upload** — CLI path wiring into pending upload store
6. **SharePoint media** — Node 24+ compatibility fix

## Proposed Pattern: Unified MS Teams Auth Module

```typescript
// src/channels/msteams/auth/msteams-auth-module.ts

export type TeamsCredentialType = 
  | 'SingleTenant' 
  | 'MultiTenant' 
  | 'Federated_Certificate'
  | 'Federated_ManagedIdentity';

export interface TeamsAuthState {
  credentialType: TeamsCredentialType;
  token: string | null;
  expiresAt: number;
  refreshToken: string | null;
  scopes: string[];
}

export class MSTeamsAuthModule {
  private state: TeamsAuthState;
  
  constructor(config: MSTeamsChannelConfig) {
    // Auto-detect credential type from config
    this.state = this.initializeState(config);
  }
  
  async getToken(scopes: string[]): Promise<string> {
    // Unified token acquisition with refresh logic
    // Handles all credential types transparently
  }
  
  async withDelegatedAuth<T>(action: string, fn: (token: string) => Promise<T>): Promise<T> {
    // Wraps actions with appropriate auth context
    // Automatically selects delegated vs app-only based on action type
  }
}
```

## State Machine for Auth Lifecycle

```
[Uninitialized] → [Detecting] → [Configured]
                                    ↓
                        ┌───────────┴───────────┐
                   [SingleTenant]          [Federated]
                        ↓                       ↓
                   [AppOnly]              [Certificate/MI]
                        ↓                       ↓
                   [Ready]                 [Ready]
                        └───────┬───────────────┘
                                ↓
                          [Refreshing]
                                ↓
                          [Ready/Expired]
```

## Fix History (April 2026)

| Commit | Issue | Fix |
|--------|-------|-----|
| `53615` | #53615 | Federated credential support (certificate + managed identity) |
| `51646` | #51646 | Reaction support with delegated auth and pagination |
| `57530` | #57530 | Group management actions (add/remove participant, rename) |
| `64645` | #64645 | Channel file attachments broken by HTML fallback |
| `64641` | #64641 | SingleTenant `sts.windows.net` JWT issuer validation |
| `64646` | #64646 | CLI --media path wiring into pending upload store |
| `64652` | #64652 | SharePoint media fetch fails on Node 24+ |

## Recommendations

1. **Consolidate auth into single module** — All credential types flow through one `MSTeamsAuthModule`
2. **Add auth type detection** — Auto-detect SingleTenant vs Federated from config
3. **Delegated auth for user-context actions** — Reactions, group management need user tokens
4. **App-only auth for bot-context actions** — File uploads, notifications can use app tokens
5. **Node 24+ compatibility** — SharePoint media fetch needs updated HTTP client

## Related References

- `service_lifecycle_pattern.md` — Two-phase startup pattern
- `oauth_scope_preservation.md` — OAuth scope lifecycle management
- `auth_pattern_audit_v2.md` — Auth pattern audit across channels