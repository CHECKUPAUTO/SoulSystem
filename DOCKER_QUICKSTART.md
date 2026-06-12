# SoulSystem — Démarrage rapide avec Docker

## Prérequis

- Docker et Docker Compose installés
- Un token de bot Telegram (obtenu via [@BotFather](https://t.me/BotFather))

## Build et lancement

```bash
# Cloner le dépôt
git clone https://github.com/CHECKUPAUTO/SoulSystem.git
cd SoulSystem

# Configurer le token Telegram (depuis un fichier .env ou un gestionnaire de secrets)
# NE PAS hardcoder le token dans ce fichier
export TELEGRAM_BOT_TOKEN="${TELEGRAM_BOT_TOKEN}"

# Build et lancement
docker compose up --build
```

Le dashboard développeur est accessible sur http://localhost:9090.

## Obtenir un token Telegram

1. Ouvrez Telegram et contactez [@BotFather](https://t.me/BotFather)
2. Envoyez `/newbot`
3. Suivez les instructions pour nommer votre bot
4. Copiez le token et utilisez-le dans la variable `TELEGRAM_BOT_TOKEN`

## Arrêter

```bash
docker compose down
```
