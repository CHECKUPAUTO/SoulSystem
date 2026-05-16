# Maintenance & Gouvernance

## Modèle de gouvernance

SoulSystem suit un modèle **BDFL** (Benevolent Dictator For Life) :

- **BDFL** : le créateur du projet détient l'autorité finale sur toutes les
  décisions techniques et stratégiques.

## Mainteneurs de modules

Chaque module majeur a un ou plusieurs mainteneurs responsables :

| Module      | Mainteneur(s)    | Responsabilités                  |
|-------------|------------------|----------------------------------|
| OpenEvolve  | À désigner       | Évolution automatique, sandbox   |
| SciRust     | À désigner       | Calcul scientifique, GPU         |
| AVID        | À désigner       | Recherche, parsing arXiv         |
| SYNERGIE    | À désigner       | Détection de synergies           |
| Clawd       | À désigner       | Assistant Telegram, UX           |

## Devenir mainteneur

1. **Historique de contributions** : au moins 5 PRs fusionnées dans le module
   concerné.
2. **Proposition** : un mainteneur existant ou le BDFL propose le candidat.
3. **Vote** : les mainteneurs actuels votent. Majorité simple requise.
4. **Période d'essai** : 3 mois comme co-mainteneur avant pleine
   responsabilité.

## Responsabilités des mainteneurs

- **Review de PRs** : chaque PR doit être revue par au moins un mainteneur.
- **Gestion des issues** : triage, assignation, fermeture.
- **Documentation** : maintenir la documentation du module à jour.
- **Stabilité** : s'assurer que `cargo test` passe dans le module.

## Décisions majeures (RFC)

Pour tout changement majeur (nouveau module, changement d'API publique,
modification de la gouvernance) :

1. Rédiger une **RFC** (Request For Comments) dans `docs/rfc/`.
2. Période de commentaires : 2 semaines minimum.
3. Le BDFL tranche en dernier ressort.

## Processus pour proposer un changement majeur

1. Ouvrir une issue avec le tag `rfc-proposal`.
2. Décrire le problème, la solution proposée, et l'impact.
3. Si l'issue reçoit un soutien, rédiger une RFC complète.
4. Soumettre la RFC en PR dans `docs/rfc/`.
5. Après discussion et approbation, implémenter.

## Contact

Pour toute question de gouvernance, contacter le BDFL via les issues GitHub.
