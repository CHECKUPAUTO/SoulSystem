# SoulSystem Threat Model

Scope: the `soulsystem` runtime, its network listeners, its tool-execution and
memory subsystems, and its self-modification path. Baseline commit:
`9d2f82783d87c3dad50eade02ce2c96d90c628f5`.

This is a living document. It is derived from a fresh reading of the current
code, not copied from the older static audit
(`docs/audit/SOULSYSTEM_FULL_AUDIT_2026-07-21.md`, baseline `5e3b0c3b`), which
is treated as a lead and re-verified in [`findings.json`](findings.json).

## Assets

| Asset | Why it matters |
|-------|----------------|
| Host operating system and filesystem | The agent can execute processes and write files; a breach means host compromise. |
| Credentials and secrets | Gateway tokens, webhook secrets, LLM API keys, signing/encryption keys. |
| Persistent memory (CCOS, planner history, vector stores, journals) | Poisoned or corrupted memory changes future agent behaviour. |
| Autonomous decision integrity | The planner drives real side effects; false success signals cause runaway behaviour. |
| Live code / deployment | Self-modification can persist attacker-controlled behaviour. |
| Availability | Resource exhaustion (output floods, fork bombs) can take the host down. |

## Trust boundaries

1. **LLM output → runtime.** The model's tool selections and arguments are
   *untrusted* input. Prompt injection can make the model emit arbitrary tool
   calls.
2. **Tool output → runtime/memory.** Fetched web content, file contents, and
   subprocess output are *untrusted* and may carry injection payloads.
3. **Network client → listener.** Any client that can reach a bound port
   (loopback or beyond) is *untrusted* until authenticated.
4. **Webhook sender → provider endpoint.** Untrusted until the signature is
   cryptographically verified.
5. **Recalled memory → prompt.** Previously persisted, possibly attacker-shaped
   content is *untrusted* when re-injected into context.

## Adversaries

| Adversary | Capability |
|-----------|-----------|
| Prompt injector | Influences LLM output (via a web page, a file, a tool result) to drive tool calls. |
| Local co-tenant | Can reach loopback-bound listeners on the same host. |
| Network attacker | Reaches a non-loopback listener or a misconfigured reverse proxy. |
| Malicious webhook sender | Knows or guesses a webhook URL. |
| Supply-chain attacker | Influences a dependency or a self-modification proposal. |

## Primary attack paths (re-verified against current main)

Each maps to a finding ID in [`findings.json`](findings.json) and a remediation PR.

1. **Untrusted tool name → process execution.** If dispatch has a fallback that
   turns an unknown tool name into a shell/process invocation, prompt injection
   yields arbitrary executable execution. → PR B (reject) + PR D (isolate).
2. **Capability misclassification.** If write/patch tools are classified as
   read operations, the approval gate is bypassed. → PR C.
3. **Unsandboxed execution.** If execution does not go through OS-level
   isolation, a permitted command still has full filesystem/network access.
   → PR D.
4. **Arbitrary file write.** If file tools accept caller-supplied absolute paths,
   the agent can overwrite sensitive files. → PR E.
5. **Unauthenticated state-changing endpoints.** If listeners lack auth, any
   reachable client triggers execution/goals/memory changes. → PR F, PR G.
6. **Weak webhook verification.** If signatures are optional when secrets are
   unset, spoofed payloads drive the agent. → PR F.
7. **Persist-before-screen.** If tool output is stored before injection
   screening, later recall re-injects the payload. → PR H.
8. **False planner success.** If history records every tool as successful, the
   planner cannot react to failures. → PR I.
9. **Secret exposure in memory/logs.** Non-zeroized keys and secrets in logs.
   → PR J.
10. **Unrestricted self-modification.** Direct writes of generated code without
    signing/approval persist malicious behaviour. → PR K.
11. **Persistence corruption.** Non-atomic writes corrupt causal memory on
    crash. → PR L.
12. **Resource exhaustion.** Unbounded output/processes exhaust host resources.
    → PR D, PR G.

## Immediate containment (PR A)

Until the Critical paths above are fixed, `SOULSYSTEM_ENV=production` **fails
closed**: the `soul-prod-guard` startup guard refuses to start when auth
material, an active TLS path for non-loopback binds, a canonical workspace root,
the isolation backend, or a safe self-modification policy is missing, and when
default/example secrets or insecure file permissions are present. Development
mode surfaces the same findings as warnings. See
[`SECURITY_INVARIANTS.md`](SECURITY_INVARIANTS.md) INV-ENV-\*.

## Out of scope (this effort)

- Physical attacks and cold-boot attacks beyond zeroization best-effort.
- Hardening of the GPU sub-workspace internals (excluded from audit scope).
- Third-party LLM provider security posture.
