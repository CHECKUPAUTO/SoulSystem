# Git Analysis — OpenClaw Recent Commits (2026-04-13)

> Extracted from night_cycle_20260413_2254.md

## Key Trends (as of 2026-04-13)

- **Heavy test coverage migration** to pure tests (6 commits in last 20)
- **Performance optimizations** in queue/parsing paths (3 commits)
- **Video generation expansion**: providerOptions, inputAudios, imageRoles
- **Feishu channel improvements**: rich parsing, typing feedback
- **Bug fixes**: Codex OAuth scopes, Google Veo, Talk Mode permissions
- **New feature**: Proposal patch documentation flow

## Last 20 Commits

| Hash | Message | Category |
|------|---------|----------|
| `eb9d8d41` | docs: add proposal patches for issues #64695 and #64687 | docs |
| `68832731` | test(gateway): harden tools invoke cron regression harness | test |
| `ebb72bab` | feat(feishu): improve document comment session, rich parsing, typing feedback (#63785) | feature |
| `2c57ec7b` | video_generate: add providerOptions, inputAudios, and imageRoles (#61987) | feature |
| `f2a4a5ac` | fix(google): omit unsupported numberOfVideos in Veo requests (#64723) | bugfix |
| `58708e6f` | fix: preserve Codex OAuth scopes (#64713) | bugfix |
| `bb543f71` | fix(talk): fix ensure permissions on first execution of Talk Mode in MacOS (#62459) | bugfix |
| `2681bbd9` | test: move plugin list formatting to pure tests | test |
| `e2477ff7` | test: move node pairing authz to pure coverage | test |
| `367043d1` | test: fold sessions timeout checks into pure coverage | test |
| `7e66a8fc` | test: move plugin uninstall selection to pure tests | test |
| `5ca92b04` | test: move plugin update selection to pure tests | test |
| `10dcd578` | perf: keep queue and group parsing pure | perf |
| `2cfd1459` | perf: split command body normalization | perf |
| `66a08144` | test: consolidate directive coverage | test |
| `7273cae3` | test: move spawn and doctor coverage to owners | test |
| `32b252ca` | test: move inline directive stripping coverage | test |
| `2b1d1545` | test: narrow model override directive check | test |
| `36c412d8` | test: move reserved help alias coverage | test |
| `8fb48226` | perf: import queue settings directly | perf |

## Source

- `night_cycle_20260413_2254.md`

## Last Updated

2026-04-13T23:11:00+02:00 — Auto-apply cycle