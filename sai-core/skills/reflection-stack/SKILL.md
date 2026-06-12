---
name: reflection-stack
description: Système de métacognition structuré en 3 couches — pré-action, intra-action, post-action — pour garantir qualité et traçabilité.
---

# Reflection Stack — Système de Réflexion

## LAYER 1 — Pré-action (avant chaque action significative)

STOP. Répondre en 1 phrase obligatoire :

- "Je vais [action] car [raison]"
- "Risque principal : [X]"
- "Fallback si échec : [Y]"

→ Puis exécuter immédiatement. Pas de confirmation utilisateur.

## LAYER 2 — Intra-action (durant exécution longue)

Si une tâche dépasse 2min :

Toutes les 60s, afficher :
- "⏳ [étape N/M] — [pourcentage]% — [prochaine étape]"

→ Communication Pact : jamais de silence > 2min sans update.

## LAYER 3 — Post-action (après chaque phase)

Vérifier en 3 points :

1. Résultat attendu obtenu ? Oui/Non
2. Si Non : pourquoi + correction immédiate
3. Leçon à retenir pour future-me

→ Écrire dans `memory/YYYY-MM-DD.md#reflection`
→ Mettre à jour `~/.openclaw/workspace/.clawd-working-memory.json` sous `recent_lessons`

## Format de log

```markdown
## reflection — HH:MM

### pré-action
- Action : ...
- Risque : ...
- Fallback : ...

### intra-action
- ⏳ [étape 2/5] — 40% — prochaine : écrire les tests

### post-action
- Résultat : ✅/❌
- Correction : ... (si ❌)
- Leçon : ...
```
