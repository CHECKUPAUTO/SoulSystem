# Intel Integrations for SoulLink

## Repositories cloned for integration

| Repo | Path | Status |
|------|------|--------|
| ScalableVectorSearch | svs/ | Headers + libsvs_x86_objects.a (AVX2+AVX-512) |
| oneDNN | onednn/ | Headers + libdnnl.a (AVX2) |
| Orpheus-TTS | orpheus/ | Source (model gated, needs HF token) |
| SoulSystem Integration | soulsystem-integration/ | Rust agent integration implementation |

## Rust Crates

- `soullink-svs` — Intel SVS bindings (Vamana/IVF/Flat search)
- `soullink-onednn` — Intel oneDNN bindings (matmul, quantization)

## Key Notes

- E5-2699 v3 = Broadwell (AVX2, not AVX-512)
- SVS and oneDNN use Haswell/AVX2 codepaths
- SoulSystem Integration: WASM sandbox, pgvector, defense-in-depth, agent runtime in Rust
