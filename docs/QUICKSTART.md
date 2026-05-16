# SoulSystem — Guide d'Installation Rapide (Debian 12)

Ce guide vous permet d'installer et de lancer SoulSystem sur une machine
Debian 12 (Bookworm) en quelques minutes.

## Prérequis

- Debian 12 (Bookworm) fraîchement installé
- Au moins 8 Go de RAM, 20 Go d'espace disque
- Connexion Internet

## 1. Installer les dépendances système

```bash
sudo apt update
sudo apt install -y curl build-essential pkg-config libssl-dev nftables
```

## 2. Installer Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
```

## 3. Installer Ollama

```bash
curl -fsSL https://ollama.com/install.sh | sh
```

Démarrez le serveur Ollama et téléchargez un modèle léger :

```bash
ollama serve &
ollama pull tinyllama
```

## 4. Cloner SoulSystem

```bash
git clone https://github.com/CHECKUPAUTO/SoulSystem.git
cd SoulSystem
```

## 5. Compiler

```bash
cargo build --release
```

La compilation prend quelques minutes. Le binaire principal se trouve dans
`target/release/soulsystem`.

## 6. Configuration minimale

Créez le fichier de configuration par défaut :

```bash
mkdir -p /opt/soulsystem/config /var/lib/soulsystem/data /var/log/soulsystem
cp soulsystem.toml /opt/soulsystem/config/
```

Le fichier `soulsystem.toml` contient les chemins par défaut :

```toml
[paths]
config_dir = "/opt/soulsystem/config"
data_dir = "/var/lib/soulsystem/data"
log_dir = "/var/log/soulsystem"
```

## 7. Configurer le bot Telegram (Clawd Assistant)

1. Ouvrez Telegram et contactez [@BotFather](https://t.me/BotFather)
2. Envoyez `/newbot` et suivez les instructions
3. Notez le **token** fourni (ex: `123456:ABC-DEF1234ghikl...`)

Exportez le token :

```bash
export TELEGRAM_BOT_TOKEN="votre_token_ici"
```

## 8. Lancer SoulSystem

```bash
./target/release/soulsystem
```

Le système démarre :
- Le kernel OpenClaw-U
- Le mesh neuronal SoulLink HNN
- L'assistant Clawd connecté à Telegram
- Les modules optionnels (AVID, OpenEvolve, SciRust, SYNERGIE) ne sont **pas**
  lancés par défaut.

## 9. Interagir avec Clawd

Ouvrez Telegram, cherchez votre bot et commencez à discuter. Clawd répond
via le modèle tinyllama local.

## Modules optionnels

Les modules suivants sont disponibles mais non activés par défaut :

| Module      | Dépôt                                          | Rôle                    |
|-------------|------------------------------------------------|-------------------------|
| AVID        | https://github.com/CHECKUPAUTO/AVID            | Ingénierie & recherche  |
| OpenEvolve  | https://github.com/CHECKUPAUTO/openevolve      | Évolution automatique   |
| SciRust     | https://github.com/CHECKUPAUTO/scirust         | Calcul scientifique     |
| SYNERGIE    | https://github.com/CHECKUPAUTO/SYNERGIE        | Détection de synergies  |

Pour les activer, clonez les dépôts dans le dossier parent de SoulSystem
et redémarrez.

## Résolution de problèmes

- **Ollama ne démarre pas** : vérifiez que le service tourne avec `ollama ps`
- **Erreur de compilation** : exécutez `rustup update` puis réessayez
- **Clawd ne répond pas** : vérifiez `export | grep TELEGRAM_BOT_TOKEN`
