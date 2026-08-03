# ═══════════════════════════════════════════════
# SoulSystem Evolution v0.2 — Makefile
# ═══════════════════════════════════════════════

.PHONY: build up down logs status restart clean shell check-ollama backup snapshots

build:
	@echo "🔨 Build SoulSystem v0.2..."
	docker compose build

up: check-ollama
	@echo "🚀 Lancement SoulSystem v0.2..."
	docker compose up -d
	@echo "✅ Container lancé — make logs pour suivre"

down:
	@echo "🛑 Arrêt..."
	docker compose down

restart: down up

logs:
	docker compose logs -f --tail=100

status:
	@echo "── Container ──"
	@docker compose ps
	@echo ""
	@docker inspect soulsystem-evolution --format='Health: {{.State.Health.Status}}' 2>/dev/null || true
	@echo ""
	@docker stats soulsystem-evolution --no-stream 2>/dev/null || true

shell:
	docker compose exec soulsystem-evo bash

check-ollama:
	@curl -sf http://localhost:11434/api/tags > /dev/null 2>&1 && \
		echo "✅ Ollama OK" || \
		(echo "❌ Ollama non disponible sur localhost:11434" && exit 1)

clean:
	@echo "🧹 Nettoyage complet..."
	docker compose down -v --rmi local
	@echo "✅ Nettoyé"

backup:
	@mkdir -p backups
	docker cp soulsystem-evolution:/opt/soulsystem/data ./backups/data_$$(date +%Y%m%d_%H%M%S)
	@echo "✅ Backup dans ./backups/"

snapshots:
	docker compose exec soulsystem-evo ls -lah data/snapshots/
