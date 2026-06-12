# SoulSystem — Makefile de validation et maintenance
#
# Usage:
#   make validate       # Validation complète (check + test + clippy)
#   make validate-fast  # cargo check uniquement
#   make test           # Tests unitaires tous projets
#   make pre-commit     # Active le hook pre-commit
#   make ci             # Pipeline CI complet

.PHONY: help validate validate-fast test pre-commit ci

.DEFAULT_GOAL := help

help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

validate: ## Validation complète (check + test + clippy + stubs)
	scripts/validate.sh

validate-fast: ## cargo check uniquement sur tous les projets
	scripts/validate.sh --fast

validate-chronos: ## Validation rapide chronos-agent uniquement
	scripts/validate.sh --fast --project chronos-agent

test: ## Tests unitaires tous projets
	@echo "=== SoulSystem ==="
	cargo test --lib -p soulsystem 2>&1 | tail -3
	@echo "=== chronos-agent ==="
	cargo test --manifest-path scirust-chronos-agent/Cargo.toml 2>&1 | tail -3
	@echo "=== chronos-agent --lib ==="
	cargo test --manifest-path scirust-chronos-agent/Cargo.toml --lib 2>&1 | tail -3

check: ## cargo check tous projets
	@echo "=== SoulSystem ==="
	cargo check -p soulsystem 2>&1 | tail -3
	@echo "=== chronos-agent ==="
	cargo check --manifest-path scirust-chronos-agent/Cargo.toml 2>&1 | tail -3

clippy: ## cargo clippy
	scripts/validate.sh --project chronos-agent

pre-commit: ## Active le hook pre-commit
	@if [ -f .git/hooks/pre-commit ]; then \
		echo "✅ Pre-commit hook already installed"; \
	else \
		echo "Installing pre-commit hook..."; \
		cp scripts/pre-commit.sh .git/hooks/pre-commit; \
		chmod +x .git/hooks/pre-commit; \
		echo "✅ Done"; \
	fi

ci: ## Pipeline CI complet (validate + test complet)
	scripts/validate.sh
	cargo test --manifest-path scirust-chronos-agent/Cargo.toml --lib 2>&1
