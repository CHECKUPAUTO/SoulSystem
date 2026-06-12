# OpenClaw Security & Reliability Hardening Report

**Auto-generated from Night Cycle Analysis**  
**Reports:** night_cycle_20260411_0732.md, night_cycle_20260411_0745.md, night_cycle_20260411_0945.md  
**Date:** 2026-04-11

---

## Critical Issues Identified (Status Tracking)

### OpenClaw Core Issues

| Issue | Priority | Status | Commit/Ref |
|-------|----------|--------|------------|
| Gateway crashes every ~6-7 min (node:sqlite) | P0 | Open | #64695 |
| OAuth for openai-codex broken | P0 | Open | #64687 |
| Telegram requireMention: false ignored | P1 | Open | #64698 |
| Sessions send label injection bug | P1 | Open | #64699 |
| memory-wiki bridge import issues | P1 | Open | #64696 |
| Config persistence failure | P1 | Open | #64676 |
| active-memory plugin not found | P1 | Open | #64704 |

### VisionClaw Issues (Security)

| Issue | Priority | Status | Risk Level |
|-------|----------|--------|------------|
| API keys in source code | P0 | **UNFIXED** | Critical |
| No TLS enforcement | P0 | **UNFIXED** | Critical |
| No retry logic | P1 | Open | High |
| No circuit breaker | P1 | Open | High |
| No WebSocket reconnection | P1 | Open | Medium |
| CDP duplicate browser tabs | P2 | Tracking | Low |

**Note:** VisionClaw security issues from previous cycle remain unaddressed. These require immediate attention before production deployment.

---

## Security Fixes Applied (Upstream)

### Memory Leak Prevention
- **Commit:** `61e22f23dd`
- **Fix:** TTL cleanup for 3 Maps that grow unbounded causing OOM
- **Details:** Added TTL-based eviction for unbounded Map growth in gateway

### SSRF Prevention
- **Commit:** `e0b8ddc1a5`
- **Fix:** Three-phase interaction navigation guard for browser automation
- **Details:** Prevents SSRF via browser navigation

### TOCTOU Race Condition
- **Commit:** `53dbbd065c`
- **Fix:** Atomic pinned-fd open for script execution
- **Details:** Replaces check-then-read with atomic operations

### Dependency Security
- **Commit:** `9f97ad857a`
- **Fix:** Pin axios to 1.15.0, add dependency denylist
- **Details:** Security patch for axios vulnerabilities

---

## Reliability Improvements Documented

### Circuit Breaker Pattern (exec-evolved skill)
```python
from exec_core import create_circuit_breaker, exec_with_retry

# Create circuit breaker with custom thresholds
cb = create_circuit_breaker(
    'external_api',
    failure_threshold=5,     # Open after 5 failures
    recovery_timeout=60      # Try again after 60 seconds
)

# Use in retry operations
result = exec_with_retry(
    'curl https://api.example.com/data',
    max_retries=3,
    circuit_breaker=cb
)
```

**Status:** Applied to exec-evolved skill v1.1.0

### Retry Logic with Exponential Backoff
```python
result = exec_with_retry(
    "flakey_command",
    max_retries=3,
    retry_delay=1.0,
    backoff_multiplier=2.0,
    max_delay=30.0
)
```

**Features:**
- Exponential backoff (1s → 2s → 4s)
- Jitter to prevent thundering herd
- Max delay enforcement
- Attempt tracking

---

## Recommended Security Hardening

### Immediate (This Week)

1. **VisionClaw Security Remediation**
   ```swift
   // INSECURE (current):
   static let openClawGatewayToken = "your-token"
   
   // SECURE (recommended):
   import KeychainAccess
   let token = Keychain.shared.get("openclaw_token") ??
               ProcessInfo.processInfo.environment["OPENCLEW_TOKEN"]!
   ```

2. **TLS Enforcement**
   - Change `http://` to `https://` for OpenClaw Gateway
   - Implement certificate pinning
   - Add TLS requirement checks

3. **Retry Logic**
   - Exponential backoff (1s → 2s → 4s)
   - Max 3 attempts
   - Jitter to prevent thundering herd

### Short-Term (This Month)

4. **Circuit Breaker Implementation**
   - 5 failures → 30s cooldown
   - Half-open state for recovery testing
   - Per-service circuit isolation

5. **WebSocket Heartbeat**
   - Ping/pong with 30s timeout
   - Automatic reconnection on failure
   - Connection state tracking

6. **Secrets Rotation**
   - Short-lived JWT tokens
   - OAuth2 refresh token flow
   - Automatic rotation on expiry

---

## Cross-Repository Integration

### PolymathicAI/the_well Integration

**Dataset:** 15TB physics simulation datasets

**Proposed Skill:**
```typescript
@skill.command()
async queryDataset(dataset: string, field: string, trajectory: number) {
  const well = new TheWellAPI();
  const data = await well.load(dataset, { split: "train" });
  return await data[trajectory][field].visualize();
}
```

**Usage:**
- "Show me the acoustic scattering pattern for trajectory 42"
- "What does the MHD_256 simulation show at step 50?"

### VisionClaw + AION Integration

**Flow:**
```
VisionClaw (point at sky)
    ↓
"What galaxy is this?"
    ↓
Gemini Live + OpenClaw
    ↓
AION skill: legacy_survey_image → AION model
    ↓
Redshift prediction + object classification
    ↓
Gemini speaks result
```

---

## Audit Trail

### Security Events
- [x] TTL cleanup for unbounded Maps (2026-04-11)
- [x] SSRF three-phase navigation guard (2026-04-11)
- [x] TOCTOU atomic pinned-fd open (2026-04-11)
- [x] Axios security patch (2026-04-11)

### Reliability Improvements
- [x] Circuit breaker pattern (exec-evolved skill)
- [x] Execution metrics tracking
- [x] Log rotation for audit logs
- [ ] VisionClaw retry logic (PENDING)
- [ ] VisionClaw circuit breaker (PENDING)
- [ ] VisionClaw secure token storage (PENDING)

---

## Priority Action Matrix

| Priority | Action | Repository | Effort | Status |
|----------|--------|------------|--------|--------|
| 🔴 P0 | Fix node:sqlite crash | OpenClaw | Low | Open #64695 |
| 🔴 P0 | Restore OAuth | OpenClaw | Medium | Open #64687 |
| 🔴 P0 | Move API keys to Keychain | VisionClaw | Low | **UNFIXED** |
| 🔴 P0 | Enforce TLS | VisionClaw | Low | **UNFIXED** |
| 🟡 P1 | Fix Telegram requireMention | OpenClaw | Medium | Open #64698 |
| 🟡 P1 | Fix sessions_send label | OpenClaw | Medium | Open #64699 |
| 🟡 P1 | Add retry logic | VisionClaw | Medium | Open |
| 🟡 P1 | Implement circuit breaker | VisionClaw | Medium | Open |
| 🟢 P2 | CDP duplicate tabs | OpenClaw | Medium | Tracking |
| 🟢 P2 | Create the_well skill | OpenClaw | Medium | Planned |

---

## Security Checklist

### Code Security
- [x] No API keys in source code (OpenClaw)
- [ ] API keys in Keychain/Keystore (VisionClaw)
- [x] TLS enforced (OpenClaw)
- [ ] TLS enforced (VisionClaw)
- [x] TOCTOU race conditions fixed
- [x] SSRF guards implemented
- [x] Memory leak prevention (TTL cleanup)

### Operational Security
- [ ] Secrets rotation mechanism
- [ ] Audit logging (OpenClaw)
- [ ] Audit logging (VisionClaw)
- [ ] Circuit breaker for external APIs
- [ ] Rate limiting
- [ ] Timeout handling

---

*Generated by OpenEvolve Night Cycle Auto-Apply*  
*Timestamp: 2026-04-11T09:57:00Z*
