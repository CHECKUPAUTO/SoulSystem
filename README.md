# Système de Mémoire Cognitive

Ce projet implémente un système de gestion de mémoire et de communication quotidienne pour une intelligence artificielle. Il permet de stocker des connaissances, de les consolider et de générer des rapports quotidiens (briefings) et des réflexions nocturnes (rêves).

## Structure du Projet

L'arborescence du dépôt est organisée comme suit :

*   **`briefings/`** : Contient les briefings matinaux générés quotidiennement, résumant l'état des connaissances et les thèmes abordés.
*   **`dreams/`** : Répertoire stockant les "rêves" de l'IA, qui sont des synthèses créatives ou des interconnexions de connaissances formées pendant les phases de repos.
*   **`memory.json`** : Le noyau de la mémoire, structuré en paliers (tiers) :
    *   **Hot Tier** : Mémoire active contenant les concepts récents, les tenseurs associés et les synthèses.
    *   **Cold Tier** : Mémoire à long terme pour les connaissances moins fréquemment accédées.
*   **`ingest_stats.json`** : Statistiques sur l'ingestion des données, incluant le nombre de pages traitées, les erreurs rencontrées et les horodatages.

## Concepts Clés

*   **Synapses** : Unité de mesure de la connectivité et de la richesse de la mémoire.
*   **Concepts Synthétiques** : Connaissances émergentes résultant de la fusion de plusieurs fragments d'information.
*   **Consolidation** : Processus de transfert et d'organisation des données entre les différents paliers de mémoire.

## Utilisation

Ce système est conçu pour être mis à jour périodiquement par des processus d'ingestion et de consolidation, fournissant une interface textuelle via les briefings pour interagir avec son créateur.
