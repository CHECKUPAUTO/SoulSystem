# Guide de Contribution

Merci de l'intérêt que vous portez au projet **OS-AGENTS** ! Nous accueillons avec plaisir les contributions de la communauté pour améliorer ce framework de système d'exploitation cognitif.

## 🛠 Comment contribuer ?

### 1. Rapporter des bogues
Si vous trouvez un bogue, veuillez ouvrir une "Issue" sur GitHub en fournissant :
- Une description claire du problème.
- Les étapes pour reproduire le bogue.
- Votre environnement (version de Rust, OS, matériel).

### 2. Proposer des fonctionnalités
Toute idée d'amélioration est la bienvenue. Ouvrez une issue pour en discuter avant de commencer le développement.

### 3. Soumettre des modifications (Pull Requests)
1. **Forkez** le dépôt.
2. Créez une **branche** descriptive (`feature/mon-amelioration` ou `fix/mon-correctif`).
3. Appliquez vos modifications.
4. Assurez-vous que votre code respecte les conventions du projet (voir ci-dessous).
5. Lancez les tests.
6. Soumettez une **Pull Request**.

## 📏 Conventions de Code

- **Langue** :
  - Les commentaires de bas niveau et le code lui-même doivent être en **anglais**.
  - La documentation de haut niveau et les commentaires conceptuels peuvent être en **français** ou **anglais**.
- **Style** : Utilisez `cargo fmt` pour formater votre code avant de soumettre.
- **Sécurité** : Soyez extrêmement prudent avec l'utilisation de blocs `unsafe`. Ils doivent être documentés avec une section `// SAFETY:`.
- **Performance** : Ce projet est axé sur la performance. Évitez les allocations inutiles dans le "chemin chaud" (hot path).

## 🧪 Tests

Avant de soumettre une modification, assurez-vous que tout fonctionne correctement :

```bash
# Vérification globale
./check.sh

# Lancer les tests unitaires
cargo test
```

## 📜 Code de Conduite

Veuillez rester respectueux et professionnel dans toutes vos interactions au sein du projet.

---
*Merci de contribuer à l'évolution de Soul System !*
