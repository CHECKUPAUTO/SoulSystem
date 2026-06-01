# 🔗 SYNERGIE

Agent autonome de détection de synergies dans l'écosystème SoulLink.

## Architecture

```
synergie/
├── src/
│   ├── main.rs      ← Point d'entrée
│   ├── scanner.rs   ← Scan des projets
│   ├── analyzer.rs  ← Analyse des connexions
│   ├── reporter.rs  ← Génération rapports
│   └── types.rs     ← Structures de données
└── synergy_engine.py ← Implémentation Python (legacy)
```

## Détecteurs

1. **Shared dependencies** — crates/libs partagées
2. **Duplicate functions** — fonctions dupliquées
3. **API complementarity** — API complémentaires
4. **Config patterns** — patterns de config
5. **Doc references** — références croisées

## Usage

```bash
cargo run -- --scan-all
cargo run -- --report
cargo run -- --watch
```

## Service systemd

```bash
systemctl enable soullink-synergy.timer
systemctl start soullink-synergy.timer
```

Scan toutes les 6 heures.

## 🔍 Anomaly Detection

SYNERGIE intègre un détecteur d'anomalies (`src/detectors/anomaly.rs`) qui surveille :
- Chute soudaine du ticks/seconde HNN
- Boucle d'optimisation OpenEvolve divergente (score qui empire sur 10 générations)

Les anomalies sont publiées sur le bus interne et notifiées via Clawd (Telegram).
