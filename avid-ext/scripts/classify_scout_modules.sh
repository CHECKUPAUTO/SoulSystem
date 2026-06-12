#!/usr/bin/env bash
# classify_scout_modules.sh — Classe les modules avid-scout en 4 catégories.
#
# Heuristiques :
#   STUB_AVERE         : pub_fn=0 OR contenu déclare explicitement "placeholder"/"stub"
#   KEYWORD_NAIVE      : contains >= 5 (faux positifs garantis)
#   POTENTIELLEMENT_OK : contains entre 1 et 4 (à inspecter individuellement)
#   STRUCTUREL         : contains=0 et pub_fn>=1 (utilise probablement parser/regex/json)
#
# Usage : bash scripts/classify_scout_modules.sh [--detail]

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

DETAIL=0
[[ "${1:-}" == "--detail" ]] && DETAIL=1

declare -a stubs naive maybe_ok structurels

for f in crates/avid-scout/src/*.rs; do
    [[ -f "$f" ]] || continue
    name=$(basename "$f" .rs)

    # Skip lib.rs (le bloc d'allow est lui pour de bonnes raisons)
    [[ "$name" == "lib" ]] && continue

    allow_lines=$(awk '/^#!\[allow\(/,/^\)\]/{count++} END{print count}' "$f")
    [[ "${allow_lines:-0}" -le 20 ]] && continue  # pas un Jules-allow-block

    pub_fn=$(grep -cE '^pub (async )?fn ' "$f")
    contains=$(grep -c 'lower\.contains\|html\.to_lowercase()\.contains\|url_lower\.contains' "$f" || true)
    is_placeholder=$(grep -cE 'placeholder|stub|not implemented|todo' "$f" || true)

    if [[ "$pub_fn" -eq 0 ]] || [[ "$is_placeholder" -ge 2 ]]; then
        stubs+=("$name")
    elif [[ "$contains" -ge 5 ]]; then
        naive+=("$name")
    elif [[ "$contains" -ge 1 ]]; then
        maybe_ok+=("$name")
    else
        structurels+=("$name")
    fi
done

printf "═══════════════════════════════════════════════════════════════\n"
printf "  AVID Scout — Classification des modules Jules-allow-block\n"
printf "═══════════════════════════════════════════════════════════════\n\n"

printf "🔴 STUB AVÉRÉ — à réécrire ou supprimer (%s modules)\n" "${#stubs[@]}"
printf "   pub_fn=0 ou contenu déclare 'placeholder'/'stub'\n"
for m in "${stubs[@]}"; do printf "   - %s\n" "$m"; done

printf "\n🟠 KEYWORD NAÏF — faux positifs garantis (%s modules)\n" "${#naive[@]}"
printf "   contains() >= 5 occurrences = matching superficiel\n"
for m in "${naive[@]}"; do printf "   - %s\n" "$m"; done

printf "\n🟡 POTENTIELLEMENT OK — à inspecter (%s modules)\n" "${#maybe_ok[@]}"
printf "   contains entre 1 et 4 — logique mixte, à valider\n"
for m in "${maybe_ok[@]}"; do printf "   - %s\n" "$m"; done

printf "\n🟢 STRUCTUREL — probablement OK (%s modules)\n" "${#structurels[@]}"
printf "   contains=0 + au moins 1 pub_fn — utilise parser/regex/json\n"
for m in "${structurels[@]}"; do printf "   - %s\n" "$m"; done

if [[ $DETAIL -eq 1 ]]; then
    printf "\n═══════════════════════════════════════════════════════════════\n"
    printf "  Détail par module (LOC, allow, contains, pub_fn)\n"
    printf "═══════════════════════════════════════════════════════════════\n"
    for f in crates/avid-scout/src/*.rs; do
        name=$(basename "$f" .rs)
        [[ "$name" == "lib" ]] && continue
        allow_lines=$(awk '/^#!\[allow\(/,/^\)\]/{count++} END{print count}' "$f")
        [[ "${allow_lines:-0}" -le 20 ]] && continue
        loc=$(wc -l < "$f")
        pub_fn=$(grep -cE '^pub (async )?fn ' "$f")
        contains=$(grep -c 'lower\.contains\|html\.to_lowercase()\.contains\|url_lower\.contains' "$f" || true)
        printf "  %-32s LOC=%4s allow=%2s contains=%2s pub_fn=%s\n" \
            "$name" "$loc" "$allow_lines" "$contains" "$pub_fn"
    done
fi

printf "\n═══════════════════════════════════════════════════════════════\n"
printf "  Recommandations\n"
printf "═══════════════════════════════════════════════════════════════\n"
printf "  🔴 STUB_AVERE      → supprimer ou réécrire\n"
printf "                       (3 ont une PR de fix : headless, screenshot, social_meta)\n"
printf "  🟠 KEYWORD_NAIVE   → remplacer par approche DOM-aware + multi-signal\n"
printf "                       (replacements fournis pour les 3 plus utilisés)\n"
printf "  🟡 POTENTIELLEMENT  → run remove_jules_allow_blocks.sh sur eux,\n"
printf "     OK              attendre les warnings clippy, corriger ou supprimer\n"
printf "  🟢 STRUCTUREL      → run remove_jules_allow_blocks.sh, fixer les 0-3\n"
printf "                       warnings clippy qui resteront\n"
