# Inférence Quantique Hybride dans SciRust

## Pourquoi l'informatique quantique sans ordinateur quantique ?

Tu n'as pas besoin d'un ordinateur quantique pour développer, tester, et bénéficier de l'inférence quantique hybride. Voici comment.

---

## 1. Principe : Simulation Classique de Circuits Quantiques

Un ordinateur quantique manipule des **qubits** (systèmes quantiques à 2 niveaux) via des **portes quantiques**. On peut simuler cela **classiquement** avec des matrices :

```rust
// Un qubit est un vecteur de 2 nombres complexes
// |ψ⟩ = α|0⟩ + β|1⟩  où |α|² + |β|² = 1
struct Qubit {
    alpha: Complex<f64>,  // amplitude de |0⟩
    beta: Complex<f64>,   // amplitude de |1⟩
}
```

**La simulation est exponentielle :** `n` qubits nécessitent `2ⁿ` nombres complexes. Pour n=30, c'est ~16 Go de RAM — faisable sur un serveur. Mais pour n=50+, c'est irréaliste (16 Po). C'est pourquoi on a besoin de vrais ordinateurs quantiques.

## 2. Pour SciRust : Circuits Paramétrés (< 20 qubits)

Au lieu de viser 100+ qubits (impossible à simuler), on utilise des **circuits paramétrés** de 4 à 16 qubits. C'est exactement ce que fait le Quantum Machine Learning (QML).

```rust
/// Circuit quantique paramétré (simulé classiquement)
pub struct QuantumCircuit {
    qubits: usize,
    state: Vec<Complex<f64>>,  // 2^qubits amplitudes
    parameters: Vec<String>,
}

impl QuantumCircuit {
    // Porte RX: rotation autour de X
    pub fn rx(&mut self, qubit: usize, theta: f64) {
        let cos = (theta / 2.0).cos();
        let sin = (theta / 2.0).sin();
        // Applique la matrice [[cos, -i*sin], [-i*sin, cos]] au qubit
        // en modifiant les amplitudes de l'état global
    }

    // Porte CNOT: inverse la cible si le contrôle est |1⟩
    pub fn cnot(&mut self, control: usize, target: usize) {
        // Applique [[1,0,0,0],[0,1,0,0],[0,0,0,1],[0,0,1,0]]
    }

    // Mesure: probabiliste, retourne 0 ou 1
    pub fn measure(&self, qubit: usize) -> u8 {
        let p0 = self.probability_of_zero(qubit);
        if rand::random::<f64>() < p0 { 0 } else { 1 }
    }
}
```

### Exemple concret : Circuit Variational (VQE)

```
|0⟩ ── RX(θ₁) ──●──────── RX(θ₃) ──●──
                 │                  │
|0⟩ ── RX(θ₂) ──⊕──────── RX(θ₄) ──⊕──
```

Ce circuit a 4 paramètres (θ₁, θ₂, θ₃, θ₄). On peut l'optimiser avec l'algorithme génétique de `scirust-genetic` ou le reverse-mode AD de `scirust-autodiff` (via le gradient parameter-shift).

## 3. Intégration avec l'Existant

### a) Apprentissage Hybride Quantique-Classique

```rust
use scirust_inference::tensor::Tensor;
use scirust_quantum::*;

// 1. Features classiques → encodage quantique
let classical = Tensor::<f32>::rand(&[4]);
let circuit = QuantumCircuit::new(4)
    .encode_amplitude(classical.data()); // angle encoding

// 2. Circuit variationnel
circuit.rx(0, params[0]);
circuit.ry(1, params[1]);
circuit.cnot(0, 1);

// 3. Mesure → features classiques
let measurements = circuit.measure_all();

// 4. Classification classique
let model = SequentialModel::new();
model.add(Linear::new(4, 2, true));
let prediction = model.forward(&measurements)?;
```

### b) Réseau Bayésien Quantique

```rust
// Un noeud bayésien dont la CPT est générée par un circuit quantique
let qnode = BayesianNode::new_quantum("qclassifier", 4, |params| {
    let mut circuit = QuantumCircuit::new(4);
    circuit.rx(0, params[0]);
    circuit.rx(1, params[1]);
    circuit.cnot(0, 2);
    circuit.cnot(1, 3);
    circuit.measure_probability(0) // retourne P(0)
});

let mut net = BayesianNetwork::new();
net.add_node(qnode);
net.add_edge("prior", "qclassifier", 1.0);
```

### c) Optimisation avec Gradient Parameter-Shift

Le reverse-mode AD de `scirust-autodiff::tape` ne peut pas traverser directement les portes quantiques. On utilise plutôt la **règle de décalage de paramètre** (paramètre-shift rule) :

```rust
fn quantum_gradient(circuit: &QuantumCircuit, param_idx: usize, params: &[f64]) -> f64 {
    let shift = std::f64::consts::PI / 2.0;

    // Évaluer à θ + π/2
    let mut forward = circuit.clone();
    forward.set_param(param_idx, params[param_idx] + shift);
    let fwd = forward.expected_value();

    // Évaluer à θ - π/2
    let mut backward = circuit.clone();
    backward.set_param(param_idx, params[param_idx] - shift);
    let bwd = backward.expected_value();

    // Gradient = (f(θ+π/2) - f(θ-π/2)) / 2
    (fwd - bwd) / 2.0
}
```

## 4. Ce qui est Faisable AUJOURD'HUI (simulation classique)

| Nombre de qubits | Mémoire nécessaire | Temps par simulation | Cas d'usage |
|---|---|---|---|
| 4 | 256 octets | < 1 µs | Classification basique, XOR |
| 8 | 4 Ko | < 10 µs | Feature maps, encodage |
| 12 | 64 Ko | < 1 ms | Circuits variationnels (VQE) |
| 16 | 1 Mo | < 10 ms | QAOA pour optimisation |
| 20 | 16 Mo | < 100 ms | Quantum kernels |
| 24 | 256 Mo | ~ 1 s | Recherche (Grover simulé) |
| 28 | 4 Go | ~ 10 s | Limite pratique sur serveur |

## 5. Architecture Proposée

```
scirust-quantum/
├── src/
│   ├── lib.rs           # Point d'entrée, QuantumModule trait
│   ├── circuit.rs       # QuantumCircuit (portes RX, RY, RZ, CNOT, etc.)
│   ├── simulator.rs     # Simulation par multiplication matricielle
│   ├── gradient.rs      # Parameter-shift rule
│   ├── encoding.rs      # Angle/amplitude encoding de données classiques
│   └── hybrid.rs        # Interface avec scirust-inference et scirust-probability
```

## 6. Contraintes et Limitations

1. **Nombre de qubits limité** : 24-28 max sur un serveur standard (plus avec GPU via cuQuantum)
2. **Pas de correction d'erreur** : La simulation suppose des portes parfaites
3. **Bruit quantique simulable** : On peut ajouter du bruit de dépolarisation pour plus de réalisme
4. **Pas d'avantage quantique** : La simulation classique est nécessairement plus lente qu'un algorithme classique équivalent. L'avantage quantique viendrait d'un vrai QPU.

## 7. Et avec un Vrai Ordinateur Quantique ?

Quand tu auras accès à un vrai QPU (IBM Quantum, AWS Braket, etc.), la seule chose à changer est le backend :

```rust
// Mode simulation (aujourd'hui)
let circuit = QuantumCircuit::new(8).simulate();

// Mode réel (demain, avec un QPU)
let circuit = QuantumCircuit::new(8)
    .backend(Backend::IBMQ("ibm_sherbrooke"))
    .run();
```

L'API reste identique. Tout le code d'inférence bayésienne, de cohérence, et d'apprentissage reste inchangé.

## 8. Résumé

**Sans ordinateur quantique**, tu peux :
- Développer et tester des algorithmes hybrides jusqu'à ~24 qubits
- Entraîner des circuits variationnels avec GA et parameter-shift
- Intégrer des "noeuds quantiques" dans les réseaux bayésiens
- Publier et démontrer le concept

**Quand tu en auras un**, tu remplaceras juste le simulateur par le vrai backend.

C'est exactement la stratégie qu'utilisent IBM-Qiskit, PennyLane, et Google Cirq.
