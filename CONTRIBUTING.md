# Contributing to JIT Agentic Engine

First of all, thank you for considering contributing to JIT Agentic Engine! It's people like you that make JIT Agentic Engine such a great tool.

## How to Contribute

### Reporting Bugs

- Check the issues to see if the bug has already been reported.
- If you can't find an open issue that describes the problem, open a new one.
- Include a clear title and description, as much relevant information as possible, and a code sample or an executable test case demonstrating the expected behavior that is not occurring.

### Suggesting Enhancements

- Open a new issue with a clear title and description of the suggested enhancement.
- Explain why this enhancement would be useful to most JIT Agentic Engine users.

### Pull Requests

1. Fork the repository.
2. Create a new branch for your feature or bug fix: `git checkout -b feature/your-feature-name` or `git checkout -b bugfix/your-bug-fix-name`.
3. Make your changes.
4. Ensure your code follows the existing style and all tests pass.
5. Commit your changes: `git commit -m "Add some feature"`.
6. Push to the branch: `git push origin feature/your-feature-name`.
7. Submit a pull request.

## Development Setup

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (latest stable version recommended)
- `cargo`

### Running Tests

To run the entire workspace tests:

```bash
cargo test --workspace
```

To run the integration demo:

```bash
cargo run -p jit_demo
```

## Coding Conventions

- Follow standard Rust naming conventions (`snake_case` for functions/variables, `PascalCase` for structs/enums).
- Use `anyhow` for error handling in applications and integration points.
- Ensure all public functions are documented if they add significant complexity.
- All new features should ideally come with a corresponding test case or demo.

## License

By contributing, you agree that your contributions will be licensed under its MIT License.
