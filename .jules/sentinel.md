# Sentinel Journal - Critical Security Learnings

This journal records critical security learnings and vulnerability patterns discovered in the SoulSystem ecosystem.

## 2025-05-15 - Initial Journal Setup
**Vulnerability:** N/A
**Learning:** Initializing the Sentinel security journal.
**Prevention:** N/A

## 2025-05-15 - Command Injection and Path Traversal in Autonomous Actions
**Vulnerability:** `Action::execute` in `soul-kernel was vulnerable to command injection and path traversal because it passed unsanitized strings from potentially untrusted sources (LLM, remote downlink) to system commands (`systemctl`, `iptables`, `sh -c`) and file operations.
**Learning:** Even when using `Command::new().args()`, passing arbitrary strings to certain utilities can be dangerous (e.g., flag injection in `iptables` or path traversal in file writes). Whitelist-based validation is essential for autonomous agents.
**Prevention:** Always validate inputs to system actions using strict whitelists (alphanumeric for names, standard parsers for IPs). Avoid absolute paths in workspace `Cargo.toml` files to ensure build portability and avoid leaking internal structure.
