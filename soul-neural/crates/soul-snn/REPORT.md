# Rapport Technique Soul_Spike (Module ANN-to-SNN)

## 1. Rapport Mathématique & Conceptuel

### Modèle de Neurone LIF (Leaky Integrate-and-Fire)
Le module implémente la dynamique discrète du neurone LIF pour orchestrer le traitement asynchrone. L'équation de mise à jour du potentiel de membrane $V_m$ est :
$$V_m(t+1) = V_m(t) \cdot \left(1 - \frac{1}{\tau}\right) + X(t)$$
Où :
- $V_m(t)$ est le potentiel à l'instant $t$ ($f32$).
- $\tau$ est la constante de temps de fuite.
- $X(t)$ est le courant d'entrée ($f32$ activation issue de l'ANN).

Dès que $V_m(t) \geq V_{thresh}$, un spike est émis et $V_m$ est réinitialisé à $V_{reset}$.

### Protocoles de Codage (Bridge Layer)
Le `BridgeLayer` permet la conversion des activations continues en trains d'impulsions binaires via deux méthodes :

1.  **Rate Coding (Codage en Fréquence)** : L'intensité de l'activation ($f32$) est traduite en une probabilité de spike sur une fenêtre temporelle $T$. Une activation de 0.8 génère statistiquement 80% de spikes sur l'intervalle.
2.  **Time-to-First-Spike (Codage par Latence)** : Maximise la parcimonie. Une forte activation déclenche un spike immédiat, tandis qu'une faible activation retarde l'émission. La latence $L$ est calculée par : $L = (1 - \text{activation}) \cdot (T - 1)$.

## 2. Architecture et Dépendances

Le module est structuré en trois composants principaux :
- `lif.rs` : Coeur de la dynamique neuronale utilisant des primitives $f32$.
- `bridge.rs` : Encodage et interface ANN-SNN (Rate & Latency encoders).
- `lib.rs` : Orchestration asynchrone via `tokio::sync::mpsc`.

**Dépendances principales :**
- `ndarray` : Manipulation efficace des vecteurs d'activation.
- `tokio` : Runtime asynchrone pour l'orchestration événementielle (canaux `mpsc`).
- `rand` : Génération stochastique pour le rate coding.

## 3. Métriques de Performance Théoriques

### Complexité Algorithmique
- **Mise à jour LIF** : $O(N)$ par pas de temps, où $N$ est le nombre de neurones.
- **Encodage par Latence** : $O(1)$ par neurone (calcul direct de l'index).
- **Encodage par Fréquence** : $O(T)$ par neurone pour une fenêtre $T$.

### Efficience Énergétique et Parcimonie
L'utilisation de `AsyncSpikeOrchestrator` basé sur `tokio::sync::mpsc` permet une propagation événementielle. Au lieu de multiplier des matrices denses à chaque cycle, le système ne traite que les événements "Spike", transformant les opérations $f32$ coûteuses en mises à jour de potentiels ciblées, minimisant l'activité CPU inutile.

### Localité des Données
Les structures `LIFNeuron` et les vecteurs de potentiels sont optimisés pour l'alignement mémoire, favorisant l'utilisation des registres vectoriels SIMD lors des phases de traitement par batch.
