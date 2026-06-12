# 🤖 AVID — Exemple de Boucle Autonome (Agentic Loop)

Depuis la mise à jour v2.1, AVID peut fonctionner de manière totalement autonome en utilisant le nouveau `DiscoveryAgent`. Voici comment lancer une session d'exploration et de clonage automatique.

## 1. Démarrer le serveur AVID

```bash
# Assurez-vous qu'Ollama est lancé
cargo run --bin avid-server
```

## 2. Lancer une tâche de découverte initiale

Au lieu d'une simple description, envoyez une URL cible. AVID explorera cette URL, en extraira la logique, et cherchera de nouvelles cibles.

```bash
curl -X POST http://localhost:3000/tasks \
  -H "Authorization: Bearer dev-token" \
  -H "Content-Type: application/json" \
  -d '{
    "task": "Explore cette documentation d'\''API et clone les endpoints de base",
    "url": "https://api.example.com/docs"
  }'
```

## 3. Ce qui se passe en coulisses (L'Organisme en Action)

### Étape A : Exploration (`Scout`)
AVID télécharge la page et toutes les ressources liées. Il extrait le texte brut et les liens.

### Étape B : Analyse Multi-Moteurs
- **Vision** : Identifie que la page contient une structure de documentation Swagger/OpenAPI.
- **Cortex** : Lit les descriptions textuelles pour comprendre que c'est un système de gestion de stocks avec authentification JWT.

### Étape C : Clonage (`Mimic`)
AVID génère automatiquement une implémentation Python (`inventory_api.py`) qui réplique le comportement décrit.

### Étape D : Découverte (`DiscoveryAgent`) 🚀 **NOUVEAU**
Pendant qu'il traite la tâche, le `DiscoveryAgent` analyse les autres liens trouvés sur `api.example.com`.
- Il repère un lien vers `https://api.example.com/webhooks`.
- Il crée **automatiquement** une nouvelle tâche dans la file d'attente : *"Implémenter le système de Webhooks détecté"*.

## 4. Surveiller l'évolution

Vous pouvez voir l'organisme "réfléchir" et s'auto-assigner des tâches dans les logs :

```text
INFO orchestrator: worker 0 processing task: scout-https://api.example.com/docs
INFO orchestrator: running autonomous discovery for task 123...
INFO discovery: Agent found interesting target: https://api.example.com/webhooks
INFO orchestrator: task enqueued: 456 (Suggested by DiscoveryAgent)
```

## 5. Résultat final

En laissant AVID tourner, vous obtenez non seulement le clone de l'API initiale, mais aussi une couverture complète de l'écosystème lié (webhooks, SDKs, outils internes) que l'agent a découvert de lui-même.
