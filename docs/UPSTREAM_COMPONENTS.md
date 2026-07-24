# Upstream component provenance

SoulSystem vendors selected components so a normal clone builds without
absolute local paths, private checkout state, or nested Git repositories.

| Component | Upstream | Imported revision | Integration |
|---|---|---|---|
| SoulSystem base | `Memorithm/SoulSystem` | `b3974aeb55687fb694067b4adada865ba24e30b8` | Root repository baseline |
| SciRust | `Memorithm/scirust` | `25f272a0506c9e67dd15051f1ac3235bfdd13e3d` | Selected crates plus their local path-dependency closure |
| CCOS | `Memorithm/CCOS` | `aaa941df0e54c5f8d4bf9b11a1797565d55331dc` | `ccos/`, with SoulSystem concurrency and durable-persistence adaptations |
| CERVO | `Memorithm/cervo` | `bd1ef1687158774f57454d3a687bd6379819e4b0` | Vendored in `cervo/`; no machine-specific symlink |

The SciRust integration includes `scirust-core`, `scirust-simd`,
`scirust-autodiff`, `scirust-gpu`, `scirust-learning`, `scirust-symbolic`,
`scirust-reasoning`, and the local crates required by their current manifests.
GPU-only crates remain standalone manifests and are validated individually by CI.

CCOS upstream uses single-threaded interior-mutability caches. SoulSystem
replaces those cache cells with synchronized locks because its autonomous
runtime shares causal memory across Tokio tasks. SoulSystem also retains
atomic, fsync-backed persistence for crash-safe snapshots.

Update this file whenever a vendored component is refreshed. A component
refresh is incomplete until `cargo metadata`, focused component tests, the root
CLI build, and security checks all pass.
