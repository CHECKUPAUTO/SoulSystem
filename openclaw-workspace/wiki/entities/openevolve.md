# OpenEvolve

_Entity page — the evolutionary code reviewer._

## Overview
- **Type**: Evolutionary code review engine (Rust)
- **Formerly**: IronReview v4.0 + T430 algorithm
- **Binary**: /usr/local/bin/openevolve → /usr/local/lib/openevolve/openevolve
- **Install**: /mnt/nvme_secondary/ai_projects/.openclaw/workspace/openevolve-rust/

## Version History
- IronReview v3.0 → v4.0 (merged with T430)
- OpenEvolve v4.0 (unified engine, 2026-04-11)
- Naming: `T430Evolution` → `OpenEvolveEngine`, crate `ironreview` → `openevolve`

## Night Cycle
- Dual-model analysis (gemma4:31b + kimi-k2.5)
- Catches security issues missed by single-model review
- Runs nightly at ~4 AM
- Proposals rated by fitness (top: 93.5%)

## Auto-Apply Rules
- Documentation-only changes: safe, applied automatically
- Code changes: require explicit approval
- Security-sensitive: documented, not applied without PR

## See Also
- [rust-migration](../concepts/rust-migration.md)