# Documentation Technique du Système de Mémoire

## 1. Architecture de la Mémoire (`memory.json`)

La mémoire est organisée de manière hiérarchique pour optimiser l'accès et la rétention des informations.

### Hot Tier (Palier Actif)
Contient les informations récemment ingérées ou fréquemment consultées.
- **Structure d'un objet :**
    - `tensor`: Représentation vectorielle de l'information (utilisée pour la recherche sémantique).
    - `text`: Contenu textuel brut ou synthétisé.
    - `title`: Identifiant ou titre du concept.
    - `hits`: Nombre de fois où l'information a été accédée.
    - `timestamp`: Date de la dernière mise à jour.
    - `synthesized_from`: Référence aux fragments originaux.

### Cold Tier (Palier de Stockage)
Stocke les informations moins prioritaires, archivées après consolidation.

---

## 2. Statistiques d'Ingestion (`ingest_stats.json`)

Ce fichier assure le suivi des performances du processus d'ingestion.
- `pages_ingested`: Nombre total de pages traitées.
- `errors`: Nombre d'erreurs rencontrées durant le processus.
- `start_time`: Horodatage du début de l'ingestion (format Unix).
- `last_update`: Date et heure de la dernière mise à jour (format ISO 8601).

---

## 3. Flux Opérationnel

Le système suit un cycle continu pour maintenir sa base de connaissances :

1.  **Ingestion** : Des données externes sont transformées en tenseurs et ajoutées au `Hot Tier`.
2.  **Consolidation** : Un processus périodique analyse les `hits` et le `timestamp` pour déplacer les données vers le `Cold Tier` ou fusionner des informations redondantes en **Concepts Synthétiques**.
3.  **Rêve (Dream)** : Phase de traitement nocturne où le système génère des connexions créatives entre les concepts, stockées dans `/dreams`.
4.  **Briefing** : Synthèse matinale générée pour l'utilisateur, récapitulant les nouvelles connaissances et l'état des synapses.

---

## 4. Terminologie Technique

- **Synapse** : Représente une connexion ou une unité de connaissance au sein du système.
- **Tenseur** : Vecteur numérique permettant au système de comprendre le contexte mathématique d'une information.
- **Concept Synthétique** : Résultat d'une fusion intelligente de plusieurs informations liées.
