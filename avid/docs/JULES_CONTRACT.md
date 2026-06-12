# Contrat pour l'agent Jules sur le repo AVID

Toute PR ouverte par Jules DOIT respecter ces règles. Une PR qui en
viole une seule sera fermée sans review.

## Règles dures (gating automatique en CI)

1. **Pas de fichier .rs orphelin** : chaque nouveau fichier `crates/*/src/xxx.rs`
   doit être déclaré dans le `lib.rs` du crate, soit directement
   (`pub mod xxx;`), soit transitivement via un `mod.rs`. Le job CI
   `no-orphan-files` bloque toute PR qui en contient.

2. **Pas de boilerplate identique** : un script CI compare chaque
   nouveau fichier avec ceux déjà présents. Si la similarité
   structurelle (fingerprint AST via `avid-anticlone`) dépasse 0.85
   contre n'importe quel fichier existant, la PR est rejetée.

3. **Tests non-triviaux** : tout nouveau `pub fn`, `pub struct`, `pub trait`
   exposé doit avoir au moins UN test qui n'est pas juste un cas vide
   + un cas littéral matchant l'input. La review humaine vérifie ça.

4. **Pas de `#![allow(...)]` en cascade** :
   - Aucun fichier ne peut commencer par un bloc `#![allow(clippy::xxx, ...)]`
     de **plus de 5 lints distincts**.
   - Si plus de 5 lints doivent être désactivés, c'est qu'il y a un
     problème de fond avec le code, pas avec clippy.
   - Le job CI `no-jules-allow-blocks` (cf. `.github/workflows/ci.yml`)
     compte les lignes du premier `#![allow(...)]` de chaque fichier
     ajouté/modifié et échoue si > 5.
   - Pour passer la règle proprement : corriger le code, ou utiliser
     `#[allow(clippy::xxx)]` ciblé sur les fonctions/items concernés
     (jamais au niveau crate).

5. **Lignes < 200 par fichier** : sauf raison documentée. Au-delà,
   découper.

6. **Pas de keyword-matching naïf comme heuristique métier** :
   les modules dont la logique principale est une chaîne
   `html.to_lowercase().contains("foo")` × 10+ sont rejetés. Pour
   détecter des frameworks, signatures de CMS, etc., utiliser au
   minimum :
   - une vraie inspection DOM (via `scraper` ou tree-sitter HTML)
   - des headers HTTP (`X-Powered-By`, `Server`)
   - des signatures de fichiers (`/wp-content/`, `/_next/static/`)
   - et combiner au moins 2 signaux avant de conclure.
   Un module qui se contente de keyword-matching est rejeté.

7. **Pas de chemins absolus dans Cargo.toml** : `path = "/foo/bar"`
   ne marche que sur la machine de son auteur. Soit publier sur
   crates.io, soit vendorer dans `vendor/`, soit utiliser un
   submodule git. Le job CI `no-absolute-paths` grep les Cargo.toml.

## Règles de format
- Edition 2021, MSRV 1.88
- `cargo fmt --all` propre
- Documentation `///` sur tout `pub`
- Commits conventionnels (`feat(scope): ...`)

## Règles métier
- Pas de nouveau crate sans justification d'usage dans un autre crate
  du workspace
- Pas de nouvelle dépendance externe sans mention dans le commit
- Pas de réécriture massive d'un fichier existant (>50% des lignes
  changées) sans issue dédiée discutant la refonte

## Règles de merge

- Lors de l'intégration d'une branche Jules conflictuelle avec une
  autre PR récente, **ne pas utiliser `git merge -X ours` ou
  `keep HEAD` global** : ça écrase silencieusement le travail mergé
  entretemps. Résoudre fichier par fichier ou demander à Tarek.
- Une PR Jules qui touche un fichier modifié dans les 5 PRs précédentes
  doit explicitement lister ces interactions dans sa description.

## Comment vérifier en local avant de pousser

```bash
# 1. Compile
cargo check --workspace

# 2. Lints stricts
cargo clippy --workspace -- -D warnings

# 3. Tests
cargo test --workspace

# 4. Pas d'orphelins
bash scripts/find_orphan_modules.sh | (! grep -q .)

# 5. Pas d'allow-blocks géants (> 5 lints)
for f in $(git diff --name-only --diff-filter=A origin/main...HEAD | grep '\.rs$'); do
    lines=$(awk '/^#!\[allow\(/,/^\)\]/{count++} END{print count}' "$f")
    [[ "${lines:-0}" -gt 6 ]] && echo "VIOLATION rule 4: $f ($lines lines of #![allow])" && exit 1
done

# 6. Pas de chemins absolus dans Cargo.toml
grep -rn 'path = "/' crates/*/Cargo.toml && echo "VIOLATION rule 7" && exit 1
```
