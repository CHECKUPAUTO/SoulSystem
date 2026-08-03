# OpenEvolve v4.0 — Makefile de développement
# Usage : make <cible>

CARGO    := cargo
BINARY   := target/release/openevolve
EVAL_BIN := examples/my_task/target/release/evaluator

.PHONY: all build build-eval build-all test fmt lint audit clean run run-quick \
        check install help

## Cible par défaut
all: build-all

## ── Build ────────────────────────────────────────────────────────────────

build:
	$(CARGO) build --release

build-eval:
	$(CARGO) build --release --manifest-path examples/my_task/Cargo.toml

build-all: build build-eval
	@echo "✔ Binaires prêts :"
	@echo "  $(BINARY)"
	@echo "  $(EVAL_BIN)"

## ── Tests ────────────────────────────────────────────────────────────────

test:
	$(CARGO) test --all
	$(CARGO) test --manifest-path examples/my_task/Cargo.toml

test-verbose:
	$(CARGO) test --all -- --nocapture

## ── Qualité du code ──────────────────────────────────────────────────────

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all --check

lint:
	$(CARGO) clippy --all-targets --all-features -- -D warnings

audit:
	$(CARGO) audit

## ── Nettoyage ────────────────────────────────────────────────────────────

clean:
	$(CARGO) clean
	$(CARGO) clean --manifest-path examples/my_task/Cargo.toml
	rm -rf openevolve_output openevolve_output_t430

## ── Exécution ────────────────────────────────────────────────────────────

check-deps: build-all
	$(BINARY) --check --evaluator $(EVAL_BIN)

run: build-all
	$(BINARY) \
		--program examples/my_task/src/solution.rs \
		--evaluator $(EVAL_BIN)

run-quick: build-all
	$(BINARY) --quick \
		--program examples/my_task/src/solution.rs \
		--evaluator $(EVAL_BIN)

run-t430: build-all
	$(BINARY) \
		--config configs/soulsystem_t430.toml \
		--program examples/my_task/src/solution.rs \
		--evaluator $(EVAL_BIN)

## ── Installation ─────────────────────────────────────────────────────────

install: build
	sudo install -m 755 $(BINARY) /usr/local/bin/openevolve
	@echo "✔ openevolve installé dans /usr/local/bin/"

## ── Aide ─────────────────────────────────────────────────────────────────

help:
	@echo "Cibles disponibles :"
	@grep -E '^##' $(MAKEFILE_LIST) | sed 's/## /  /'
	@echo ""
	@echo "Exemples :"
	@echo "  make build-all    # Compile tout"
	@echo "  make test         # Lance les tests"
	@echo "  make run-quick    # Évolution rapide (50 itérations)"
	@echo "  make lint         # Clippy strict"
