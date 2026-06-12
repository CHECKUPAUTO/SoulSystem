# Critical Issues Consolidated - April 11, 2026

**Generated:** 2026-04-11 08:26 UTC  
**Based on:** Night Cycle Reports from 2026-04-11  
**Status:** Tracking / Awaiting Resolution

---

## P0 - Critical Issues (Immediate Action Required)

### 1. OpenClaw Gateway Crash - node:sqlite Missing (#64695)

**Impact:** Complete Gateway instability, service interruption every ~6-7 minutes  
**Status:** Open, reported Apr 11, 2026  
**Risk:** Complete service failure

**Details:**
- `node:sqlite` missing in Homebrew Node.js causing periodic crashes
- Gateway becomes unstable and requires restart
- Affects all connected channels and sessions

**Recommended Fix:**
```typescript
// Startup dependency check
async function checkDependencies(): Promise<boolean> {
  try {
    require('node:sqlite');
    return true;
  } catch (e) {
    logger.error('node:sqlite not available, falling back to bundled sqlite3');
    // Fallback to sqlite3 package
    return false;
  }
}
```

**Upstream:** Track in OpenClaw repository, fix in progress

---

### 2. OpenClaw OAuth for openai-codex Broken (#64687)

**Impact:** Cannot use Codex models via OAuth  
**Status:** Open, marked as regression  
**Risk:** Authentication failures for Codex users

**Details:**
- OAuth authentication failing for openai-codex provider
- Likely related to recent auth token refresh changes
- Regression from previous working state

**Recommended Fix:**
- Review recent auth token refresh changes
- Add regression test for OAuth flow
- Verify token refresh mechanism

**Upstream:** Track in OpenClaw repository

---

### 3. VisionClaw Security - API Keys in Source Code

**Impact:** Keys committed to git history, exposed in decompiled apps  
**Status:** ❌ UNFIXED from previous cycle  
**Risk:** Critical - token exposure, unauthorized access

**Details:**
- `Secrets.swift` and `Secrets.kt` copied from `.example` files
- Hardcoded tokens in source code
- No Keychain/Keystore integration

**Recommended Fix:**
```swift
// INSECURE (current):
static let openClawGatewayToken = "abc123..."

// SECURE (required):
import KeychainAccess
let keychain = Keychain(server: "https://openclaw.ai", protocolType: .https)
let token = try keychain.getString("openclaw_token") ??
            ProcessInfo.processInfo.environment["OPENCLEW_TOKEN"]!
```

**Action Required:** VisionClaw repository access needed

---

### 4. VisionClaw Security - No TLS Enforcement

**Impact:** Token interception on local network  
**Status:** ❌ UNFIXED from previous cycle  
**Risk:** Critical - man-in-the-middle attacks

**Details:**
- Uses `http://` for OpenClaw Gateway connections
- README shows: `static let openClawHost = "http://Your-Mac.local"`
- No certificate pinning

**Recommended Fix:**
- Enforce HTTPS for all Gateway connections
- Implement certificate pinning
- Update documentation to use `https://`

---

## P1 - High Priority Issues

### 5. Telegram requireMention: false Ignored (#64698)

**Impact:** Security/privacy issue - bot responding when it shouldn't  
**Status:** Open, reported Apr 11, 2026  
**Risk:** Unintended message responses

**Details:**
- Group message configuration not respected
- Configuration precedence logic broken

**Recommended Fix:**
- Review configuration precedence logic
- Add test coverage for requireMention

---

### 6. Sessions send Label Injection Bug (#64699)

**Impact:** Cross-session messaging broken  
**Status:** Open, marked as bug:behavior  
**Risk:** Mutual-exclusion error with sessionKey

**Details:**
- `sessions_send` unexpectedly injects label
- Causes mutual-exclusion error when used with sessionKey

**Recommended Fix:**
- Review label injection logic
- Ensure backward compatibility

---

### 7. Memory-Wiki Bridge Import Issues (#64696)

**Impact:** Memory/wiki functionality unreliable  
**Status:** Open, reported Apr 11, 2026  
**Risk:** Data loss, import failures

**Details:**
- Relative links mishandled
- Reply tags not processed
- Malformed cached source pages

**Recommended Fix:**
- Refactor bridge import with proper URL validation
- Add error handling for edge cases

---

### 8. Gateway Configuration Persistence (#64676)

**Impact:** User settings lost across restarts  
**Status:** Open, reported Apr 11, 2026  
**Risk:** Manual reconfiguration required

**Details:**
- Configuration changes not persisting
- Requires atomic write operations

**Recommended Fix:**
- Review config save mechanism
- Implement atomic write with rollback

---

### 9. VisionClaw Reliability - No Retry Logic

**Impact:** Failed OpenClaw calls fail permanently  
**Status:** ❌ UNFIXED from previous cycle  
**Risk:** Poor user experience, permanent failures

**Details:**
- No exponential backoff
- No automatic recovery

**Recommended Fix:**
```swift
// Exponential backoff retry
func callWithRetry<T>(_ operation: () async throws -> T) async throws -> T {
    let delays = [1.0, 2.0, 4.0] // seconds
    for (attempt, delay) in delays.enumerated() {
        do {
            return try await operation()
        } catch {
            if attempt == delays.count - 1 { throw error }
            try await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))
        }
    }
    fatalError("Unreachable")
}
```

---

### 10. VisionClaw Reliability - No Circuit Breaker

**Impact:** Repeated failures cascade without isolation  
**Status:** ❌ UNFIXED from previous cycle  
**Risk:** Cascading failures, system overload

**Recommended Fix:**
- 5 failures → 30s cooldown
- Graceful degradation with fallback

---

## P2 - Medium Priority Issues

### 11. Active-Memory Plugin Package Not Found (#64704)

**Impact:** New plugin system integration broken  
**Status:** Open, reported Apr 11, 2026  
**Risk:** Package unavailable on npm or ClawHub

**Recommended Fix:**
- Publish package or update documentation

---

### 12. Embedded Runs Resolve model=default (#64705)

**Impact:** Wrong model being used for Codex operations  
**Status:** Open, reported Apr 11, 2026  
**Risk:** Model resolution priority issue

---

### 13. Teams Channel sendPayload (#64690)

**Impact:** Teams integration incomplete  
**Status:** Open, feature request/bug  
**Risk:** Missing interactive approval cards

---

### 14. False Provenance Warnings (#64686)

**Impact:** User confusion, false positives  
**Status:** Open, reported Apr 11, 2026  
**Risk:** Incorrect warnings in `openclaw status`

---

## Summary Statistics

| Severity | Count | OpenClaw | VisionClaw | The Well |
|----------|-------|----------|------------|----------|
| 🔴 P0 Critical | 4 | 2 | 2 | 0 |
| 🟡 P1 High | 6 | 4 | 2 | 0 |
| 🟢 P2 Medium | 4 | 4 | 0 | 0 |
| **Total** | **14** | **10** | **4** | **0** |

---

## Action Items

### Immediate (This Week)

1. [ ] **OpenClaw Core:** Fix node:sqlite crash (#64695)
2. [ ] **OpenClaw Core:** Restore openai-codex OAuth (#64687)
3. [ ] **VisionClaw:** Move API keys to Keychain/Keystore
4. [ ] **VisionClaw:** Enforce TLS for Gateway connections

### Short Term (Next 30 Days)

5. [ ] **OpenClaw:** Fix Telegram requireMention (#64698)
6. [ ] **OpenClaw:** Fix sessions_send label bug (#64699)
7. [ ] **OpenClaw:** Fix memory-wiki bridge (#64696)
8. [ ] **VisionClaw:** Add retry logic + circuit breaker

### Long Term (Next 90 Days)

9. [ ] **All:** Unified secrets management service
10. [ ] **OpenClaw:** Comprehensive regression test suite
11. [ ] **VisionClaw:** Production security audit

---

## References

- Night Cycle Reports in `/root/.openclaw/workspace/evolution/`
- OpenClaw Issues: https://github.com/openclaw/openclaw/issues
- VisionClaw Repository: Intent-Lab (private access required)

---

*Generated by OpenEvolve Auto-Apply*  
*Classification: Documentation - Safe to Apply*
