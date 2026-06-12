# Commands for the AVID workspace.
#   just build
#   just test
#   just lint
#
# Install: cargo install just
# Docs: https://github.com/casey/just

default:
    @just --list

# ── Build ────────────────────────────────────────────────────────────────

# Build workspace (debug)
build:
    cargo build --workspace

# Build workspace (release, lto=fat)
release:
    cargo build --release --workspace

# cargo check (fast)
check:
    cargo check --workspace

# ── Test ─────────────────────────────────────────────────────────────────

# Run all tests (debug)
test:
    cargo test --workspace

# Run all tests (release)
test-release:
    cargo test --release --workspace

# ── Lint ─────────────────────────────────────────────────────────────────

# Run clippy (pedantic + nursery)
lint:
    cargo clippy --workspace -- -D warnings

# Format all code
fmt:
    cargo fmt --all

# Check formatting (CI mode)
fmt-check:
    cargo fmt --all -- --check

# ── Docs ─────────────────────────────────────────────────────────────────

# Generate rustdoc
docs:
    cargo doc --workspace --no-deps --document-private-items

# Open docs in browser
docs-open:
    cargo doc --workspace --no-deps --document-private-items --open

# ── Quality gate ─────────────────────────────────────────────────────────

# Full quality gate: fmt check + build + lint + test
all: fmt-check build lint test

# Full gate in release mode
all-release: fmt-check release lint test-release

# ── Run ──────────────────────────────────────────────────────────────────

# Start the server (requires .env file)
run:
    @test -f .env || { echo "ERROR: .env file not found. Copy .env.example first."; exit 1; }
    bash run.sh

# ── Clean ────────────────────────────────────────────────────────────────

# Remove build artifacts
clean:
    cargo clean

# ── Install ──────────────────────────────────────────────────────────────

# Install system dependencies and build
install:
    bash install.sh

# ── Coverage ─────────────────────────────────────────────────────────────

# Run with tarpaulin (if installed)
coverage:
    cargo tarpaulin --workspace --skip-clean --timeout 120
