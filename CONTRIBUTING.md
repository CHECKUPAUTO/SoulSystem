# Contributing to SoulSystem

## Code Style
- Rust edition 2021/2024, nightly features used
- Follow standard Rust formatting (`cargo fmt`)
- Add tests for all new functionality
- Keep documentation in English

## Git Workflow
1. Create a feature branch from `main`
2. Make your changes with descriptive commit messages
3. Run `cargo test` and `cargo clippy` before committing
4. Create a pull request with a clear description

## Continuous Integration
The project uses GitHub Actions with the following pipeline:
1. **Check**: Verify compilation
2. **Test**: Run all unit and integration tests
3. **Clippy**: Lint check with clippy
4. **Format**: Verify code formatting

## Project Structure
- `src/` — Root binary crate (soulsystem)
- `soul_*/` — Autonomous entity crates (LLM, planner, tools, REPL)
- `soul-*/` — Infrastructure crates (memory, daemon, sandbox, etc.)
- `soullink-brain/` — Neural mesh crates (HNN, memory hierarchy, MoE, etc.)
- `scirust-*/` — Scientific computing framework
- `crates/` — Shared tooling (dashboard, TUI, chaos testing)
- `docs/` — Documentation
- `scripts/` — Build/deploy/maintenance scripts

## Testing
- Run all tests: `cargo test --workspace`
- Run specific crate tests: `cargo test -p soul-memory`
- Check code quality: `cargo clippy --workspace`

## Need Help?
Open an issue or contact the SoulLink Mesh Team.