# Changelog

## v2.0.0 (2026-05-06)

### Ajouts
- `SKILL.md` racine — skill maître SAI pour l'écosystème Codex
- `tests/integration/test_orchestrator.sh` — 8 tests end-to-end
- `.githooks/pre-commit` — auto-verify avant chaque commit
- `Makefile` — install, test, verify, lint, clean, template
- `install.sh` — setup one-liner `curl | bash`
- `templates/project/` — template de projet SAI prêt à l'emploi
- `CHANGELOG.md` — historique des versions

### Fixes
- `verify.sh` : skip self-check (patterns internes déclenchaient faux positifs)
- `verify.sh` : skip stub check sur `.md`/`.txt`/`.rst` (tableaux markdown)
- `verify.sh` : pattern `\bXXX\b` strict pour éviter faux positifs `mktemp`

## v1.0.0 (2026-05-06)

### Création initiale
- `skills/auto-verify/verify.sh` — pipeline syntaxe/stubs/secrets/hallucination
- `skills/project-mode/SKILL.md` — cycle plan/exec/verify/deliver
- `skills/reflection-stack/SKILL.md` — métacognition pré/intra/post-action
- `skills/caveman-coding/SKILL.md` — règles caveman vs architecte
- `skills/sai-orchestrator/` — glue bash detect+verify+log
- `workspace/.clawd-working-memory.json` — mémoire active
- `memory/YYYY-MM-DD.md` — journal journalier
- `.github/workflows/verify.yml` — CI GitHub Actions
