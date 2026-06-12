# Audit Log

Immutable hash-chain audit log for all agent actions.
Uses sled + sha2 + chrono.
Stored at `/var/log/soulsystem/audit.sled`.
Each entry is chained via SHA-256 hash of the previous entry.
# Code Signing

All dynamic code loads require ed25519 signature verification.
Authorized keys stored in `~/.soulsystem/authorized_keys`.
