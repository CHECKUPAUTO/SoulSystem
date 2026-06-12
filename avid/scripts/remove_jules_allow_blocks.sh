#!/usr/bin/env bash
# remove_jules_allow_blocks.sh — Supprime le bloc `#![allow(...)]` géant
# en tête des modules avid-scout, pour laisser clippy révéler les vrais
# problèmes.
#
# Le bloc Jules typique est :
#
#   #![allow(
#       clippy::single_match,
#       clippy::match_same_arms,
#       ...
#       clippy::inefficient_to_string
#   )]
#
# Ces 32 lints désactivés cachent souvent des problèmes structurels.
# Une fois retirés, clippy se met à crier. On peut alors décider :
# corriger, ajouter `#[allow]` ciblé sur la fonction concernée, ou
# supprimer le module.
#
# Usage:
#   bash remove_jules_allow_blocks.sh [--dry-run] [--except module1,module2,...]
#
# Options:
#   --dry-run        : juste lister ce qui serait modifié
#   --except LIST    : ne PAS traiter ces modules (CSV)

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

DRY_RUN=0
EXCEPT=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run) DRY_RUN=1; shift;;
        --except) EXCEPT="$2"; shift 2;;
        *) echo "Unknown option: $1"; exit 1;;
    esac
done

# Convertir EXCEPT en regex
except_re=""
if [[ -n "$EXCEPT" ]]; then
    except_re="^(${EXCEPT//,/|})$"
fi

modified=0
skipped=0

for f in crates/avid-scout/src/*.rs; do
    [[ -f "$f" ]] || continue
    name=$(basename "$f" .rs)

    # lib.rs : on garde son bloc allow (il est légitime — 41 lints
    # gardent un fichier de 874 LOC propre)
    [[ "$name" == "lib" ]] && continue

    if [[ -n "$except_re" ]] && [[ "$name" =~ $except_re ]]; then
        printf "  [SKIP] %s (--except)\n" "$name"
        skipped=$((skipped + 1))
        continue
    fi

    # Détecter le bloc allow géant
    allow_lines=$(awk '/^#!\[allow\(/,/^\)\]/{count++} END{print count}' "$f")
    if [[ "${allow_lines:-0}" -le 6 ]]; then
        # OK selon JULES_CONTRACT (≤ 5 lints), on laisse
        continue
    fi

    if [[ $DRY_RUN -eq 1 ]]; then
        printf "  [DRY] %s — would remove %s lines\n" "$name" "$allow_lines"
        modified=$((modified + 1))
        continue
    fi

    # Retirer le bloc `#![allow(...)]` et la ligne `)]` qui le ferme.
    # On utilise awk pour la sécurité (sed multi-ligne est fragile).
    tmp=$(mktemp)
    awk '
        /^#!\[allow\(/ { skip = 1; next }
        skip && /^\)\]/ { skip = 0; next }
        skip { next }
        { print }
    ' "$f" > "$tmp"

    # Retire aussi les éventuelles lignes vides en tête
    awk 'BEGIN{started=0} /./{started=1} started{print}' "$tmp" > "$f"
    rm "$tmp"

    printf "  [FIX] %s — removed %s-line allow block\n" "$name" "$allow_lines"
    modified=$((modified + 1))
done

printf "\n%s files modified, %s skipped\n" "$modified" "$skipped"

if [[ $DRY_RUN -eq 1 ]]; then
    printf "\nRun without --dry-run to apply.\n"
else
    printf "\nNext step:\n"
    printf "  cargo clippy -p avid-scout 2>&1 | grep -c warning\n"
    printf "Then triage warnings one module at a time:\n"
    printf "  - cosmetic (line length, format args) → fix or #[allow] on the function\n"
    printf "  - real bug                            → fix the code\n"
    printf "  - module has zero value               → git rm crates/avid-scout/src/X.rs\n"
fi
