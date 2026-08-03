# UPGRADE v0.1 → v0.2 — Instructions pour l'agent

## RÉSUMÉ DES CHANGEMENTS

Cette mise à jour corrige les 4 problèmes identifiés par l'analyse de SoulLink :

### 1. SANDBOX — Les agents exécutent vraiment leur code
- **Avant** : `code_patch` stocké mais jamais compilé ni exécuté
- **Après** : Nouveau module `sandbox.rs` — chaque agent est compilé (`cargo build`) et exécuté dans un répertoire isolé. La fitness est mesurée sur le résultat réel.
- **Impact** : Le Dockerfile utilise `rust:1.86-slim-bookworm` comme image runtime (pas juste debian) car le sandbox a besoin de `cargo` pour compiler le code des agents.

### 2. MÉTRIQUES RÉELLES — Plus de floats aléatoires
- **Avant** : `performance: 0.3` modifié par `rng.gen_range(-0.1..0.15)` — tri random déguisé
- **Après** : `MeasuredMetrics` dans `agent.rs` — `compiles`, `runs_ok`, `tests_passed/total`, `compile_time_ms`, `run_time_ms`. La fitness est calculée à partir de ces métriques mesurées par le sandbox.
- **Impact** : Le prompt Ollama demande maintenant du code avec `println!("SOULSYSTEM_EVO_TESTS:{passed}/{total}")` pour que le harness puisse parser les résultats.

### 3. EMBEDDINGS SÉMANTIQUES — Le ranking utilise les vecteurs
- **Avant** : `nomic-embed-text` générait des embeddings jamais utilisés, ranking par `keyword_density` basique
- **Après** : `ranking.rs` appelle `ollama.embed()` pour chaque résultat crawlé, calcule la similarité cosinus avec l'embedding de la requête de curiosité. Le score sémantique pèse 40% du score total.
- **Impact** : Le ranking est maintenant piloté par la sémantique, pas juste par des mots-clés.

### 4. IL PILOTE LE SYSTÈME — Plus de log décoratif
- **Avant** : `ILProgram` créait des instructions symboliques jamais exécutées
- **Après** : `evolution.rs` contient `execute_il_program()` qui interprète chaque `OpCode` et exécute l'action correspondante (Search → crawler, Embed → ollama.embed, Rank → ranking sémantique, Mutate → génération LLM, Simulate → sandbox, SwitchMode → bascule FAST/DEEP, Rollback → restauration snapshot).
- **Impact** : Le méta-langage choisit la stratégie IL selon le contexte (stagnation ou pas), et le feedback de succès ajuste les poids des règles de grammaire.

### BONUS : Multi-modèle FAST/DEEP
- **Avant** : Un seul modèle avec un seul semaphore
- **Après** : 3 modèles configurables (FAST mutations, DEEP stagnation, EMBED ranking). Deux semaphores séparés. Bascule automatique FAST→DEEP si stagnation détectée (configurable).

### FIX DOCKERFILE
- Rust 1.78 → **1.86** (requis pour hashbrown v0.17.0 / edition2024)
- Ajout `touch src/main.rs` pour forcer la recompilation (fix du cache dummy)
- Runtime basé sur `rust:1.86-slim-bookworm` au lieu de `debian:bookworm-slim` (le sandbox a besoin de cargo)

---

## PROCÉDURE DE MISE À JOUR

### Étape 1 — Arrêter le container v0.1

```bash
cd /opt/soulsystem-evolution
make down
```

### Étape 2 — Backup des données existantes

```bash
make backup
# Vérifie que le backup est dans ./backups/
ls -la backups/
```

### Étape 3 — Décompresser la v0.2

```bash
# Sauvegarder l'ancienne version
cp -r /opt/soulsystem-evolution /opt/soulsystem-evolution.v1.backup

# Extraire la v0.2 par-dessus
cd /opt/soulsystem-evolution
unzip -o /chemin/vers/soulsystem-v0.2.zip
chmod +x entrypoint.sh
```

### Étape 4 — Mettre à jour la configuration

```bash
# Copier le nouveau template
cp .env.example .env

# VÉRIFIER et AJUSTER les modèles selon ce qui est installé :
nano .env
```

Les variables importantes à vérifier :
```
SOULSYSTEM_EVO_MODEL_FAST=qwen3.5:4b     # ← adapter au modèle installé
SOULSYSTEM_EVO_MODEL_DEEP=llama3:8b      # ← adapter au modèle installé  
SOULSYSTEM_EVO_MODEL_EMBED=nomic-embed-text
SOULSYSTEM_EVO_SANDBOX_ENABLED=true
```

Pour vérifier les modèles disponibles :
```bash
curl -s http://localhost:11434/api/tags | python3 -m json.tool
```

### Étape 5 — Rebuild l'image Docker

```bash
# IMPORTANT: --no-cache pour forcer un build propre
docker compose build --no-cache
```

Durée estimée : 8-12 minutes (Rust 1.86 + compilation release).

En cas d'erreur :
```bash
docker compose build --no-cache --progress=plain 2>&1 | tee build-v2.log
```

### Étape 6 — Lancer la v0.2

```bash
make up
```

### Étape 7 — Vérifier les logs

```bash
make logs
```

Sortie attendue :
```
✅ Ollama connecté
✅ Modèle qwen3.5:4b disponible
✅ Modèle llama3:8b disponible  
✅ Modèle nomic-embed-text disponible
🚀 Lancement SoulSystem Evolution v0.2...

  v0.2.0 — Évolution RÉELLE
  Sandbox | Embeddings | IL Piloté | FAST/DEEP

Configuration SoulSystem v0.2:
  Modèle FAST      : qwen3.5:4b (conc=2, ctx=2048)
  Modèle DEEP      : llama3:8b (conc=1, ctx=4096)
  Modèle EMBED     : nomic-embed-text
  Sandbox          : ON (timeout=30s)

► Nouveau système
Population: 20 agents | Mémoire: 0 | Concepts: 6

───── CYCLE 0 ─────
1. État: fitness=0.000, mode=FAST, stagnation=false
2. Stratégie IL: EVOLVE (6 instructions)
3. IL exécuté: IL[EVOLVE] 6/6 exécutées, ok=true
4. Sandbox: X/20 compilent, Y/20 tournent
5. Sélection: fitness 0.000 → 0.XXX
```

Les chiffres clés à surveiller :
- **agents compilent** : doit monter au fil des cycles (les LLM apprennent à générer du code correct)
- **agents tournent** : sous-ensemble de ceux qui compilent
- **mode** : FAST normalement, DEEP si stagnation après 10 cycles
- **IL strategy** : EVOLVE (normal), DEEP_EVOLVE ou SAFE_EVOLVE (stagnation)

### Étape 8 — Vérifier la santé

```bash
make status
```

---

## STRUCTURE DES FICHIERS v0.2

```
soulsystem-evolution/
├── Cargo.toml            # v0.2.0 + tempfile
├── Cargo.lock
├── Dockerfile            # Rust 1.86, fix cache, runtime avec cargo
├── docker-compose.yml    # env_file, healthcheck
├── entrypoint.sh         # Vérifie 3 modèles
├── Makefile              # Commandes de gestion
├── .env.example          # Config multi-modèle + sandbox
├── .dockerignore
└── src/
    ├── main.rs           # Point d'entrée v0.2
    ├── config.rs         # Config multi-modèle + sandbox + stagnation
    ├── evolution.rs      # Boucle avec IL piloté + sandbox + FAST/DEEP
    ├── agent.rs          # MeasuredMetrics + fitness réelle
    ├── sandbox.rs        # ★ NOUVEAU — compile + exécute le code agent
    ├── ollama.rs         # Multi-modèle (fast/deep/embed) + vrais embeddings
    ├── ranking.rs        # Cosine similarity sur embeddings
    ├── il.rs             # OpCode typé + ILExecutionResult
    ├── meta_language.rs  # Sélection de stratégie + feedback de succès
    ├── concept.rs        # Embeddings sémantiques + fusion par similarité
    ├── crawler.rs        # Inchangé
    ├── memory.rs         # Inchangé
    └── snapshot.rs       # Adapté (InferenceMode, agents_compiling)
```

---

## ROLLBACK vers v0.1

Si la v0.2 pose problème :

```bash
# Arrêter v0.2
cd /opt/soulsystem-evolution
make down

# Restaurer v0.1
cp -r /opt/soulsystem-evolution.v1.backup/* /opt/soulsystem-evolution/

# Rebuild et relancer
docker compose build --no-cache
make up
```

---

## DÉPANNAGE

### "Sandbox timeout" sur tous les agents
Le sandbox compile du code dans le container — ça demande de la mémoire.
```bash
# Augmenter le timeout
SOULSYSTEM_EVO_SANDBOX_TIMEOUT=60
# Ou désactiver temporairement
SOULSYSTEM_EVO_SANDBOX_ENABLED=false
```

### "Aucun agent ne compile"
C'est normal aux premiers cycles — le LLM apprend à générer du code valide. Après 5-10 cycles, le taux de compilation devrait monter car les agents qui compilent survivent et propagent leur code.

### "Mode DEEP activé trop souvent"
Augmenter le seuil de stagnation :
```bash
SOULSYSTEM_EVO_STAGNATION_THRESHOLD=20
```

### "OOM VRAM avec llama3:8b en mode DEEP"
Réduire le contexte DEEP :
```bash
SOULSYSTEM_EVO_CONTEXT_WINDOW_DEEP=2048
```
