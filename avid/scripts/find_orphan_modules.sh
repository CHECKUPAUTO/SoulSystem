#!/usr/bin/env bash
set -euo pipefail

# find_orphan_modules.sh — Liste les fichiers .rs non déclarés dans leurs crates
# v2: gestion récursive des sous-répertoires, fallback rustc --edition 2021

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

for crate_dir in crates/*/src; do
  [ -d "$crate_dir" ] || continue

  # 1. Récupère TOUTES les déclarations 'mod xxx;' ou 'pub mod xxx;' du crate (récursif)
  declared=$(find "$crate_dir" -name '*.rs' -exec grep -hE '^[[:space:]]*(pub[[:space:]]+)?mod[[:space:]]+[a-z_][a-z0-9_]*[[:space:]]*;' {} + 2>/dev/null \
    | sed -E 's/^[[:space:]]*(pub[[:space:]]+)?mod[[:space:]]+([a-z_][a-z0-9_]*)[[:space:]]*;.*/\2/' \
    | sort -u)

  # 2. Trouve tous les fichiers .rs (récursif, hors lib/main/mod)
  all_files=$(find "$crate_dir" -name '*.rs' -not -name 'lib.rs' -not -name 'main.rs' -not -name 'mod.rs' | sort)

  # 3. Pour chaque fichier, vérifie s'il est déclaré
  for f in $all_files; do
    # Calcule le stem relatif au crate_dir/src
    rel="${f#$crate_dir/}"
    stem="$(basename "$rel" .rs)"
    dir="$(dirname "$rel")"

    # Vérifie si le stem est déclaré quelque part
    if grep -qxF "$stem" <<< "$declared"; then
      continue
    fi

    # Vérification supplémentaire : le fichier est-il dans un sous-répertoire ?
    # Si dir != ".", le module parent (mod.rs ou dir.rs) doit exister et être déclaré
    if [ "$dir" != "." ]; then
      parent_stem="$(basename "$dir")"
      if grep -qxF "$parent_stem" <<< "$declared"; then
        # Le parent est déclaré, le sous-module devrait être dedans
        # Vérification rapide : chercher "mod $stem;" dans le parent
        parent_declares=$(grep -cE "^[[:space:]]*(pub[[:space:]]+)?mod[[:space:]]+${stem}[[:space:]]*;" \
          "$crate_dir/$dir/mod.rs" "$crate_dir/$dir.rs" 2>/dev/null || echo 0)
        if [ "$parent_declares" -gt 0 ]; then
          continue  # OK, déclaré transitivement
        fi
      fi
    fi

    # Fallback: vérifier via rustc si disponible
    if command -v rustc &>/dev/null; then
      crate_root="$crate_dir/lib.rs"
      [ -f "$crate_root" ] || crate_root="$crate_dir/main.rs"
      if [ -f "$crate_root" ]; then
        # Construit un fichier temporaire qui inclut tout
        tmpfile=$(mktemp)
        echo "// auto-generated orphan check" > "$tmpfile"
        echo "include!(\"$crate_root\");" >> "$tmpfile"
        if rustc --edition 2021 --crate-type lib "$tmpfile" -o /dev/null 2>/dev/null; then
          rm -f "$tmpfile"
          continue
        fi
        rm -f "$tmpfile"
      fi
    fi

    # Orphelin confirmé
    printf '%s\n' "$f"
  done
done
