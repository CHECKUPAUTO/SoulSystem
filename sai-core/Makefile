.PHONY: help install test verify lint clean

help:
	@echo "SAI — Système d'Amélioration Interne"
	@echo ""
	@echo "Commandes disponibles :"
	@echo "  make install    — Installe les hooks git + dépendances"
	@echo "  make test       — Lance les tests d'intégration"
	@echo "  make verify     — Exécute auto-verify sur tous les fichiers"
	@echo "  make lint       — Vérifie syntaxe YAML du workflow CI"
	@echo "  make clean      — Nettoie les fichiers temporaires"
	@echo "  make template   — Crée un nouveau projet SAI dans ./new-project/"

install:
	@echo "🔗 Installation des hooks git..."
	git config core.hooksPath .githooks
	@echo "✅ Hooks installés"
	@echo ""
	@echo "🔧 Vérification des dépendances..."
	@command -v python3 >/dev/null 2>&1 && echo "  ✅ python3" || echo "  ❌ python3 (requis)"
	@command -v node >/dev/null 2>&1 && echo "  ✅ node" || echo "  ⚠️ node (optionnel pour JS)"
	@command -v bash >/dev/null 2>&1 && echo "  ✅ bash" || echo "  ❌ bash (requis)"

test:
	@echo "🧪 Tests d'intégration..."
	bash tests/integration/test_orchestrator.sh

verify:
	@echo "🔍 Auto-verify sur tous les fichiers..."
	@find . -type f \( -name "*.py" -o -name "*.js" -o -name "*.ts" -o -name "*.tsx" -o -name "*.sh" -o -name "*.json" -o -name "*.md" \) -not -path "./.git/*" -not -path "./.github/*" | while read -r f; do bash skills/auto-verify/verify.sh "$$f" || exit 1; done
	@echo "✅ Tous les fichiers passent"

lint:
	@echo "📋 Vérification YAML..."
	python3 -c "import yaml; yaml.safe_load(open('.github/workflows/verify.yml')); print('  ✅ verify.yml valide')"

clean:
	@echo "🧹 Nettoyage..."
	find . -name "*.tmp" -o -name "*~" | xargs rm -f 2>/dev/null || true
	@echo "✅ Nettoyé"

template:
	@echo "📁 Création du template de projet..."
	cp -r templates/project new-project
	@echo "✅ Projet créé dans ./new-project/"
