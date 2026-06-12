# Soullink-SSM

Mamba-style State Space Model (S6) implementation in pure Rust with `ndarray`.

## Architecture

6 core modules + 4 extensions:

### Core

| Module | File | Purpose |
|--------|------|---------|
| HiPPO | `src/hippo.rs` | HiPPO-LegS, S4D, normal+low-rank matrix initialization |
| SSM Kernel | `src/ssm.rs` | Continuous SSM: ZOH discretization, matrix exp, convolution kernel |
| Selective SSM | `src/selective.rs` | S6: input-dependent B/C/Δ, multi-dim per-channel Δ |
| Parallel Scan | `src/parallel.rs` | Blelloch binary-tree scan (1D + N-dim) |
| MambaBlock | `src/layer.rs` | RMSNorm → Conv1D → SiLU → SelectiveSSM → Gate → OutProj + residual |
| MambaModel | `src/model.rs` | Embedding → [MambaBlock×N] → RMSNorm → OutputProj → logits |

### Extensions

| Extension | File | Description |
|-----------|------|-------------|
| INT8 Quantization | `src/quantization.rs` | Symmetric per-row int8 weight compression (4× memory reduction) |
| IO-Aware Tiled Scan | `src/tiled_scan.rs` | Flash-attention-style tiling with fused discretization |
| CUDA Parallel Scan | `src/cuda.rs` | GPU kernel (simulated in Rust; real GPU with `--features cuda`) |
| GQA/MQA Gating | `src/layer.rs` | `GateType::Grouped { num_groups }` for parameter-efficient gating |

## Build

```bash
cargo build                    # no CUDA
cargo build --release          # optimized build (~30x faster than debug)
cargo build --features cuda    # with CUDA kernel compilation (requires nvcc)
```

Requirements: Rust 2021 edition, `ndarray 0.16`

## Test

```bash
cargo test                          # 47 tests
cargo test --release                # tests in optimized mode
cargo test <test_name>              # single test
```

## Run Demos

```bash
cargo run --release --example mamba_demo    # extended demo (7 features)
cargo run --release --example benchmark     # scan performance benchmarks
cargo run --release --example benchmark -- --quick  # quick benchmark
```

## CUDA Support

Enable with `--features cuda`. Requires:
- CUDA Toolkit (nvcc) on PATH
- GPU with compute capability 6.0+ (Pascal+)

The `build.rs` compiles `kernels/scan.cu` via nvcc and links as a static library.
Rust fallback is used when the feature is disabled.

## Model Configuration

Key parameters in `MambaModelConfig`:
- `vocab_size`: vocabulary size
- `n_layers`: number of Mamba blocks
- `d_model`: hidden dimension
- `state_dim`: SSM state dimension (N)
- `expand_factor`: inner expansion (d_inner = d_model × expand_factor)
- `conv_kernel`: depthwise 1D convolution kernel size
- `weight_tied`: reuse embedding as output projection
- `gate_type`: `GateType::SiLU` or `GateType::Grouped { num_groups }`

## Key Design Decisions

- **No autograd/tch-rs**: all matrix ops use `ndarray` with manual forward
- **ZOH discretization**: augmented matrix [ΔA, ΔB; 0, 0] → matrix_exp
- **Matrix exp**: scaling & squaring, Taylor order 12
- **B̄ after discretization**: already incorporates B(x)·x — never multiply by x again
- **Parallel scan**: Blelloch binary tree, O(L log L), exclusive → inclusive
