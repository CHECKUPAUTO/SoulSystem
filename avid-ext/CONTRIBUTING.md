# Contributing to AVID

Thank you for your interest in contributing.

## Code of Conduct

This project follows the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md). All participants are expected to uphold it.

## Development Setup

```bash
# Clone
git clone https://github.com/CHECKUPAUTO/AVID.git
cd AVID

# Install dependencies + build
bash install.sh

# Verify everything works
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check
```

### Useful commands

```bash
just build      # Debug build
just test       # Run all tests
just lint       # Clippy
just fmt        # Format
just all        # Full quality gate
just docs       # Generate rustdoc
```

## Quality Standards

Every pull request must pass the quality gate:

- **Build:** `cargo build --workspace` — zero errors
- **Lint:** `cargo clippy --workspace -- -D warnings` — zero warnings
- **Format:** `cargo fmt --all -- --check` — standard style
- **Tests:** `cargo test --workspace` — all tests passing

## Code Conventions

### Crate-level lints

Every crate root must include:

```rust
#![forbid(unsafe_code)]
#![deny(warnings)]
#![warn(clippy::pedantic, clippy::nursery)]
```

Exception: `avid-sandbox/src/limits.rs` is the only file allowed to use `unsafe` (for `pre_exec` hooks).

### Error handling

- No `unwrap()` or `panic!()` in library code
- All fallible functions return `Result<T, E>` where `E` derives `thiserror::Error`
- Use `anyhow` only in binary entrypoints (`main.rs`), never in libraries

### Validation

- All external input types derive `garde::Validate`
- Validation is called at system boundaries (API handlers, agent outputs)

### Dependencies

- No stubs, no TODO placeholders, no `todo!()`
- Production code must compile against real APIs only
- Pin `rust-version = "1.88"` in workspace manifest

## Pull Request Process

1. Create a feature branch from `main`
2. Make your changes
3. Run `just all` to pass the full quality gate
4. Update `CHANGELOG.md` under `[Unreleased]`
5. Open a PR with a descriptive title and summary
6. Wait for CI to pass and a maintainer to review

## Commit Style

- Descriptive, present-tense, lowercase
- Prefix with scope: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`
- Keep commits focused — one logical change per commit

## License

By contributing, you agree that your contributions will be dual-licensed under MIT and Apache-2.0, the same terms as the project.
