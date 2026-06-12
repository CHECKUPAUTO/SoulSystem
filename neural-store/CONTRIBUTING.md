# Contributing to Neural Store

First off, thank you for considering contributing to Neural Store! It's people like you that make Neural Store a great tool.

## How Can I Contribute?

### Reporting Bugs
- Use GitHub Issues to report bugs.
- Include a clear title and description.
- Provide as much relevant information as possible (OS, Rust version, hardware).
- Include a reproducible example if possible.

### Suggesting Enhancements
- Open a GitHub Issue with the tag `enhancement`.
- Describe the feature and why it would be useful.

### Pull Requests
1. Fork the repo and create your branch from `main`.
2. If you've added code that should be tested, add tests.
3. If you've changed APIs, update the documentation.
4. Ensure the test suite passes (`cargo test`).
5. Make sure your code follows the Rust standard style (`cargo fmt`).

## Development Workflow

### Building
```bash
cargo build
```

### Running Tests
```bash
cargo test
```

### Benchmarking
```bash
cargo test --test simd_benchmarks -- --nocapture
```

## Style Guidelines
- Follow standard Rust naming conventions (`snake_case` for functions/variables, `PascalCase` for types).
- Use `anyhow` for error handling in the top-level API.
- Keep the `ffi` module clean and well-documented as it defines the stable boundary.
- Ensure all SIMD code has a safe scalar fallback.

## Licensing
By contributing to Neural Store, you agree that your contributions will be licensed under its MIT License.
