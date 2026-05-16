# Contribuer à SoulSystem

Merci de votre intérêt pour SoulSystem ! Ce document explique comment
configurer votre environnement et proposer des contributions.

## Environnement de développement

- **Rust** : utilisez la version stable la plus récente (voir
  `rust-toolchain.toml` si présent).
- **Clippy** : `cargo clippy --all-targets -- -D warnings` doit passer.
- **rustfmt** : `cargo fmt --all -- --check` doit passer.
- **Ollama** (optionnel) : pour tester localement avec un LLM.

### Compilation rapide

```bash
git clone https://github.com/CHECKUPAUTO/SoulSystem.git
cd SoulSystem
cargo build
```

## Règles de code

- **Pas de `unsafe`** sauf exception dûment motivée et approuvée en review.
- **Gestion d'erreurs** : utilisez `thiserror` pour les erreurs de librairie,
  `anyhow` pour les binaires.
- **Documentation** : toutes les API publiques (`pub fn`, `pub struct`, etc.)
  doivent être documentées avec `///`.
- **Tests** : exécutez `cargo test --all` avant de soumettre.

## Convention de commits

Nous suivons [Conventional Commits](https://www.conventionalcommits.org/) :

- `feat:` — nouvelle fonctionnalité
- `fix:` — correction de bug
- `refactor:` — refactorisation sans changement fonctionnel
- `docs:` — documentation
- `test:` — ajout ou modification de tests
- `chore:` — tâches de maintenance
- `ci:` — configuration CI/CD

## Processus de Pull Request

1. Créez une branche `feature/` ou `fix/` depuis `main`.
2. Implémentez vos changements en respectant les règles ci-dessus.
3. Exécutez `cargo fmt --all && cargo clippy --all-targets && cargo test --all`.
4. Ouvrez une PR avec une description claire :
   - Quel problème est résolu ?
   - Quels fichiers sont modifiés ?
   - Comment tester ?
5. Un mainteneur examinera votre PR. Les modifications peuvent être demandées.

## Code de conduite

Un code de conduite formel est à venir. En attendant, soyez respectueux,
constructifs et bienveillants dans toutes vos interactions.
