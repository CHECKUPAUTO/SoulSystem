# Security Policy

## Reporting a Vulnerability

**Do not open a public issue.** Email security concerns directly to the maintainer.

We take all security reports seriously. You will receive a response within 48 hours. We will keep you updated on progress and credit you in the advisory (unless you prefer to remain anonymous).

## Scope

AVID is designed to execute **untrusted, AI-generated code**. Security of the sandbox is paramount.

**In scope:**
- Sandbox escape (code execution outside the sandbox)
- Privilege escalation via `pre_exec` hooks
- Resource limit bypass (CPU, memory, processes)
- API authentication bypass
- Network isolation failure
- Database injection via task inputs

**Out of scope:**
- Denial of service via resource exhaustion (limits are best-effort on Linux)
- Side-channel attacks requiring physical access
- Vulnerabilities in dependencies that don't affect AVID's attack surface

## Defense-in-Depth

AVID applies multiple independent sandbox layers:

### Layer 1 — Process rlimits
| Limit | Value | Enforces |
|-------|-------|----------|
| `RLIMIT_CPU` | Configurable (soft/hard) | CPU seconds |
| `RLIMIT_AS` | Configurable | Virtual memory |
| `RLIMIT_NPROC` | Configurable | Child processes |
| `RLIMIT_FSIZE` | Configurable | File write size |
| `RLIMIT_NOFILE` | Configurable | Open file descriptors |

### Layer 2 — Kernel isolation
- `PR_SET_NO_NEW_PRIVS` — irrevocably prevents privilege escalation via setuid binaries
- `setpgid(0, 0)` — creates a new process group, enabling `killpg` to clean up all descendants
- Optional network namespace via `unshare -n` — drops all network interfaces

### Layer 3 — Orchestrator guards
- Wall timeout with `SIGKILL` to the entire process group
- stdout/stderr capped at 256KB with truncation markers
- Entrypoint validation — path must exist in the submitted file set
- No filesystem access outside the temp directory

### Layer 4 — Application security
- API token compared in constant time via the `subtle` crate
- All inputs validated with `garde` before processing
- `#![forbid(unsafe_code)]` on all crates except sandbox's `pre_exec`
- No `unwrap()` or `panic!()` in library code

## Known Limitations

- **RLIMIT_CPU** only counts CPU time, not wall time — an attacker can stall via I/O or sleep. Wall timeout is the mitigation.
- **RLIMIT_AS** on Linux only limits virtual memory, not physical. An attacker can allocate sparse mappings.
- Network namespace requires `unshare` binary and kernel support (`CONFIG_NET_NS`).

## Security Acknowledgments

We thank all security researchers who have helped improve AVID's security posture.
