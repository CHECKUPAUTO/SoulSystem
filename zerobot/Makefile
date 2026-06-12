# ZeroBot — Makefile
.PHONY: help build up down ingest test clean logs

.DEFAULT_GOAL := help

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

build: ## Build Docker images
	docker compose build

up: ## Start all services
	docker compose up -d

down: ## Stop all services
	docker compose down

restart: down up ## Restart all services

logs: ## Tail all logs
	docker compose logs -f

api-logs: ## Tail API logs
	docker compose logs -f agent-api

ingest: ## Ingest knowledge documents into ChromaDB
	docker compose run --rm agent-api python -m knowledge.ingest

test: ## Run unit tests
	docker compose run --rm agent-api python -m pytest tests/ -v --tb=short

test-local: ## Run tests locally (requires Python env)
	python -m pytest tests/ -v --tb=short

clean: ## Remove all containers, volumes, and temp files
	docker compose down -v
	rm -rf data/chromadb sessions/*.bin
	find . -type d -name __pycache__ -exec rm -rf {} + 2>/dev/null || true
	find . -type f -name '*.pyc' -delete

shell: ## Open a shell in the API container
	docker compose exec agent-api bash

toolbox-shell: ## Open a shell in the toolbox container
	docker compose run --rm toolbox bash
