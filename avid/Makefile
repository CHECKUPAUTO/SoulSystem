.PHONY: build release test lint fmt fmt-check all all-release run clean install check docs

# ── Build ────────────────────────────────────────────────────────────────

build:
	cargo build --workspace

release:
	cargo build --release --workspace

check:
	cargo check --workspace

# ── Test ─────────────────────────────────────────────────────────────────

test:
	cargo test --workspace

test-release:
	cargo test --release --workspace

# ── Lint ─────────────────────────────────────────────────────────────────

lint:
	cargo clippy --workspace -- -D warnings

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

# ── Docs ─────────────────────────────────────────────────────────────────

docs:
	cargo doc --workspace --no-deps --document-private-items

# ── Quality gate ─────────────────────────────────────────────────────────

all: fmt-check build lint test

all-release: fmt-check release lint test-release

# ── Run ──────────────────────────────────────────────────────────────────

run:
	@test -f .env || { echo "ERROR: .env file not found. Copy .env.example first."; exit 1; }
	bash run.sh

tui:
	@test -f .env || { echo "ERROR: .env file not found. Copy .env.example first."; exit 1; }
	bash run.sh tui

# ── Clean ────────────────────────────────────────────────────────────────

clean:
	cargo clean

# ── Install ──────────────────────────────────────────────────────────────

install:
	bash install.sh
