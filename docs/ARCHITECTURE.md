# SoulSystem Unified Architecture

## Monorepo Structure

SoulSystem is an industrial-grade monorepo containing:
- **Core Orchestrator**: Agent coordination logic and system heartbeat.
- **Message Bus**: Binary-serialized (`bincode`) central communication hub.
- **SciRust Ecosystem**: Full deep learning framework (autodiff, core, gpu, simd).
- **SciRust-TN**: Tensor-Train compression for high-dimensional neural states.
- **AVID Ecosystem**: Advanced digital organism for web exploration and API cloning.
- **BoundSystem**: Hardened sandbox using bubblewrap and seccomp.
- **SoulMemory**: Vector knowledge base with SciRust embeddings.

## Message Bus Specifications

Messages are serialized using `bincode` for maximum performance.
Topics include:
- `hnn.status`: Hamiltonian Neural Network energy metrics.
- `avid.clone_request`: Trigger AVID to clone a target URL.
- `synergy.detection`: Cross-module opportunistic discoveries.

## Security Model

All untrusted code (Python scripts, extracted snippets) runs within the **BoundSystem** sandbox:
1. No network access by default.
2. Resource limits (CPU, Memory, Disk).
3. System call filtering (seccomp).
4. Code signing verification for all dynamic loads.