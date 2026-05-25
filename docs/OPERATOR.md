# SoulSystem Operator Guide

## Deployment

1. **System Dependencies**: Ensure `bubblewrap`, `libssl-dev`, and `bwrap` are installed.
2. **Build**: Use `cargo build --release --workspace` to compile the entire ecosystem.
3. **Services**: Systemd units are provided in `configs/systemd/`.

## Configuration

Settings are managed via `soulsystem.toml`.
Key parameters:
- `bus.capacity`: Message buffer size.
- `llm.provider`: (Ollama | OpenAI | Anthropic).
- `sandbox.enabled`: Enable/disable BoundSystem isolation.

## Troubleshooting

- **Large Workspace**: `cargo check` may take longer due to 40+ crates.
- **GPU Acceleration**: Ensure CUDA drivers are correctly mapped if using `scirust-gpu`.
