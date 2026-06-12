# Audit Trail Imuable (Validation Organ)

**Source:** OpenEvolve Night Cycle 131 (2026-04-14 14:02)  
**Priority:** MEDIUM — fiabilité et compliance  
**Status:** Proposal (requires Validation organ implementation)

## Proposal

The Validation organ (proposal 5, port 9044) includes an immutable audit trail stored in RocksDB with SHA-256 chaining:

### Architecture

```rust
// Append-only audit log with hash chaining
struct AuditEntry {
    sequence: u64,           // Monotonically increasing
    timestamp: DateTime<Utc>,
    entry_type: AuditType,  // Verify | FactCheck | SafetyGate | CriticScore
    input_hash: [u8; 32],   // SHA-256 of input
    output_hash: [u8; 32],  // SHA-256 of output
    prev_hash: [u8; 32],    // Hash of previous entry (chain)
    verdict: Verdict,       // Pass | Fail | Warning
    details: String,        // Human-readable explanation
}

// Each entry's hash includes the previous entry's hash
// → tamper-evident chain (like a blockchain without mining)
fn compute_entry_hash(entry: &AuditEntry) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(entry.sequence.to_le_bytes());
    hasher.update(entry.timestamp.to_rfc3339().as_bytes());
    hasher.update(&entry.input_hash);
    hasher.update(&entry.output_hash);
    hasher.update(&entry.prev_hash);
    hasher.update(&entry.verdict.to_bytes());
    hasher.update(entry.details.as_bytes());
    hasher.finalize().into()
}
```

### RocksDB Column Families

| CF | Purpose | TTL |
|----|---------|-----|
| `audit_chain` | Append-only hash chain | None (immutable) |
| `audit_index` | Queryable index by type/timestamp | None |
| `audit_stats` | Aggregate statistics | None |

### Query Interfaces

```
GET  /api/validation/audit?type=&since=&until=&verdict=
GET  /api/validation/audit/verify-chain   — Verify entire chain integrity
GET  /api/validation/audit/stats           — Aggregate audit statistics
```

### Expected Benefits

- Tamper-evident audit log for all validation decisions
- Compliance-ready for security reviews
- Debugging: trace exactly why an output was approved/rejected
- Chain verification: detect if any entry was modified
- Statistics: track pass/fail rates over time