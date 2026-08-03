# Dependabot remediation — 2026-07-24

This change replaces Dependabot PR #91. That PR combined the `quinn-proto`
security update with an unplanned `rand` 0.8 to 0.10 migration and was also
behind `main`, which made the Check, Clippy, Test, and MSRV jobs fail.

## Remediated dependency graphs

- Root workspace: upgraded the Avid TUI stack so `lru` is no longer vulnerable,
  and upgraded Teloxide so `serde_with` resolves to a patched release.
- `soulsystem-gateway`: repaired its standalone manifest, selected Rustls instead
  of native TLS, and updated `rustls-webpki`, `quinn-proto`, and `rand`.
- `os-agents`: repaired stale SciRust paths and updated OpenTelemetry,
  Prometheus, protobuf, and `crossbeam-epoch`.
- `scirust-trading`: repaired missing workspace dependencies and updated
  `quinn-proto` and `quick-xml`.
- SoulSystem Integration architecture video: updated the npm lockfile and ESLint dependency;
  `npm audit` reports zero vulnerabilities.

`cargo audit` reports no security vulnerabilities in the four committed Rust
lockfiles. Remaining RustSec output is limited to explicitly tracked
unmaintained, yanked, or unsoundness warnings for which no safe upstream upgrade
is currently available.

## CI repairs found during validation

- Restored the missing `ccos::shield` public module export.
- Migrated the Teloxide callback API used by `clawd`.
- Updated stable-Clippy findings exposed by the dependency refresh.
- Replaced the invalid out-of-tree `workspaces/gpu` Cargo workspace with
  independent checks for each standalone GPU/CUDA manifest.

## Validation

The replacement PR runs the repository's exact Format, Check, Clippy, Test,
`cargo-deny`, GPU/CUDA, and Rust 1.93 MSRV commands. Focused checks also cover
`soulsystem-gateway` and the upgraded `soul_telemetry` package.

Two older source-level defects remain outside the CI and Dependabot scope:
`scirust-trading-engine/src/shadow.rs` contains an unmatched closing brace, and
other packages in that standalone workspace have unresolved API imports. They
do not affect dependency resolution or the vulnerability audit and should be
handled in a dedicated functional repair.
