# Security Policy

## Reporting a vulnerability

Please report security issues privately (e.g. via a GitHub security advisory)
rather than opening a public issue. Do not include live credentials in reports.

## Secret handling

- **Never commit real secrets.** Bot tokens, API keys, passwords, private keys,
  and personal data (chat IDs, phone numbers) must never be committed.
- Real environment files are git-ignored (`*.env`, except `*.env.example`).
  Commit only `*.example` templates with placeholder values.
  - Root template: [`.env.example`](.env.example)
  - SoulLink template: [`configs/env/soullink.env.example`](configs/env/soullink.env.example)
- Load secrets at runtime from environment variables, not from checked-in files.
- Known example/fixture credentials (used by the `soullink-security` scanner
  and by tests) are excluded from secret scanning via
  [`.github/secret_scanning.yml`](.github/secret_scanning.yml).

## ⚠️ Action required — rotate the previously committed Telegram bot token

A real Telegram bot token was committed to `configs/env/soullink.env` and is
therefore present in this repository's **git history**. Removing the file from
the current tree (done in this change) does **not** remove it from history, so
the credential must be treated as compromised and rotated:

1. In Telegram, message **@BotFather** and run `/revoke` for the affected bot.
   This immediately invalidates the leaked token.
2. `/token` (or `/newbot`) to obtain a fresh token.
3. Put the new token in your local, un-tracked `configs/env/soullink.env`
   (copy from `configs/env/soullink.env.example`) or your deployment's
   environment — never back into git.

Until the token is revoked, anyone who has seen the history can control the
bot. Any GitHub secret-scanning alert for this token will remain open until the
token is revoked (GitHub then reports it as inactive) or the alert is dismissed.

The same applies to any other credential or personal data that appeared in the
committed `configs/env/soullink.env` (Telegram chat/user IDs, WhatsApp number):
review and rotate/reset as appropriate.
