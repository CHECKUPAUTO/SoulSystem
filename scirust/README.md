# SciRust — Scientific Inference Engine in Rust

[![Rust](https://img.shields.io/badge/rust-1.75+-blue.svg)]()
[![Edition](https://img.shields.io/badge/edition-2021-purple.svg)]()
[![Tests](https://img.shields.io/badge/tests-165-passing-green.svg)]()

SciRust is a **scientific inference engine** built from scratch in Rust. It combines symbolic mathematics, probabilistic reasoning, neural network inference, genetic optimization, and simulated quantum computing into a unified framework.

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                        scirust-core                          │
│                  (Facade + Error types)                       │
└───┬─────┬──────┬──────┬──────┬──────┬──────┬──────┬──────────┘
    │     │      │      │      │      │      │      │
    ▼     ▼      ▼      ▼      ▼      ▼      ▼      ▼
 ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌──────────┐
 │sym-│ │auto│ │rea-│ │lea-│ │pro-│ │inf-│ │gen-│ │ quantum  │
 │bolic│ │diff│ │son-│ │rn- │ │babi│ │eren│ │etic│ │ (simul.) │
 │     │ │    │ │ing │ │ing │ │lity│ │ce  │ │    │ │          │
 └────┘ └────┘ └────┘ └────┘ └────┘ └────┘ └────┘ └──────────┘
    │      │      │                                   │
    └──────┴──────┴──────────┬────────────────────────┘
                             ▼
                    ┌──────────────────┐
                    │  scirust-bridge   │
                    │  (NLP → Command)  │
                    └──────────────────┘
                             ▼
                    ┌──────────────────┐
                    │  scirust-cli      │
                    │  (REPL + script)  │
                    └──────────────────┘
```

### Crates

| Crate | Tests | Description |
|-------|-------|-------------|
| **scirust-symbolic** | 5 | Expr AST, parser, simplifier, symbolic differentiation, evaluation, codegen |
| **scirust-autodiff** | 21 | Reverse-mode AD (Wengert tape), forward-mode (Dual), SGD/Adam optimizers |
| **scirust-reasoning** | 3 | solve_linear, solve_quadratic, Newton, trig identities, prove_equal |
| **scirust-learning** | 2 | PatternMemory, polynomial fit, linear regression, feature discovery |
| **scirust-probability** | 44 | BayesianNet (exact + Gibbs), consistency engine, hypothesis loop, inference memory |
| **scirust-inference** | 37 | Tensor NDArray, NN layers (Linear, Conv2d, BatchNorm, activations), quantization, pipeline, training |
| **scirust-genetic** | 10 | GA, multi-objective optimization (MOO), constraints, symbolic regression (GP) |
| **scirust-quantum** | 31 | Simulated quantum circuits, parameter-shift gradient, encoding, hybrid pipeline |
| **scirust-distributed** | 12 | TCP model server/client, data parallelism, protocol |
| **scirust-bridge** | 4 | NLP parser → NaturalCommand (Infer, Solve, Simplify, etc.) |
| **scirust-simd** | 5 | SIMD kernels (add, mul, f32/f64), JIT cache, `#[simd]` macro |
| **scirust-core** | 0 | Unified error types, facade |
| **scirust-cli** | 0 | REPL interface |
| **scirust-gpu** | 0 | GPU dispatch (rayon-based, placeholder for real GPU) |
| **scirust-macros** | 0 | `#[autodiff]` proc-macro |
| **scirust-simd-macros** | 0 | `#[simd]` proc-macro |
| **scirust-gpu-macros** | 0 | `#[gpu]` proc-macro |

## Quick Start

### Build

```bash
cargo build --release
```

### Run tests

```bash
cargo test --offline     # 165+ tests, all pass
```

### REPL

```bash
cargo run -p scirust-cli
```

### Inference

```rust
use scirust_inference::layers::*;
use scirust_inference::pipeline::SequentialModel;
use scirust_inference::tensor::Tensor;

let mut model = SequentialModel::new();
model.add(Linear::new(784, 256, true));
model.add(ReLU::new());
model.add(Linear::new(256, 128, true));
model.add(Sigmoid::new());
model.add(Linear::new(128, 10, false));

let input = Tensor::<f32>::rand(&[32, 784]);
let output = model.forward(&input).unwrap();
```

### Bayesian Inference

```rust
use scirust_probability::prelude::*;

let mut net = BayesianNetwork::new();
net.add_node(BayesianNode::new_continuous("rain", Prior::Beta(2.0, 5.0)));
net.add_node(BayesianNode::new_continuous("wet_grass", Prior::Beta(1.0, 1.0)));
net.add_edge("rain", "wet_grass", 0.9);

let engine = BayesianInferenceEngine::new(net);
let result = engine.query("wet_grass", &HashMap::new()).unwrap();
```

### Quantum Circuit

```rust
use scirust_quantum::circuit::QuantumCircuit;
use scirust_quantum::simulator::probabilities;

let mut qc = QuantumCircuit::bell_state();
let probs = probabilities(&qc).unwrap();
// P(00) = 0.5, P(11) = 0.5
```

### Distributed Inference

```rust
use scirust_distributed::server::ModelServer;
use scirust_distributed::client::ModelClient;

// Server
let mut server = ModelServer::new(model, "127.0.0.1:0").unwrap();
server.start().unwrap();

// Client
let mut client = ModelClient::connect(server.addr()).unwrap();
let result = client.predict(&input).unwrap();
```

## Features by Domain

### Symbolic Mathematics (`scirust-symbolic`, `scirust-reasoning`)
- Expression AST with variables, constants, arithmetic, trig, exp, ln
- Parser from string → Expr
- Algebraic simplification
- Symbolic differentiation
- Numerical evaluation
- Rust code generation
- Linear/quadratic equation solving
- Trigonometric identity application

### Probabilistic Inference (`scirust-probability`)
- **Prior distributions**: Uniform, Gaussian, Beta, Gamma, Custom
- **Posterior**: MAP estimation, Bayes factor, credible interval
- **BayesianNetwork**: DAG with CPTs, exact inference (enumeration), approximate (Gibbs), belief propagation
- **ConsistencyEngine**: Direct, indirect, and rule contradictions; cycle detection; resolution suggestion
- **HypothesisLoop**: LLM → verify → correct → max N iterations
- **InferenceMemory**: A* search, path reconstruction, pruning

### Neural Network Inference (`scirust-inference`)
- **Tensor**: N-dimensional array with SIMD GEMM, broadcast, softmax
- **Layers**: Linear, Conv2d, BatchNorm, ReLU, Sigmoid, Tanh, GELU
- **Quantization**: FP16, INT8 per-channel
- **Graph**: computation graph, FLOP estimation, activation fusion
- **Pipeline**: SequentialModel, InferenceSession, benchmarking
- **Training**: NNTrainer with tape-based autodiff, SGD/Adam

### Genetic Algorithms (`scirust-genetic`)
- **GA**: population, tournament selection, elitism, fitness-based evolution
- **MOO**: multi-objective with Pareto front, constraints (pain function)
- **GP**: symbolic regression, subtree crossover/mutation

### Quantum Simulation (`scirust-quantum`)
- **Circuit**: H, X, Y, Z, RX, RY, RZ, CNOT, CZ, SWAP
- **Simulator**: state-vector (up to 20 qubits), measurement, expectation values
- **Gradient**: parameter-shift rule, full gradient, shot-noise simulation
- **Encoding**: angle, dense-angle, amplitude, basis
- **Hybrid**: quantum-classical pipeline, Bayesian CPT bridge, variational optimizer

### Distributed Inference (`scirust-distributed`)
- **Server**: TCP model server with concurrent connections
- **Client**: TCP model client with connection pooling
- **Protocol**: length-prefixed JSON messages
- **Parallel**: data sharding + multi-threaded inference with speedup

## License

MIT OR Apache-2.0
### WASM Compatibility

The core crates (scirust-symbolic, scirust-autodiff, scirust-reasoning, scirust-learning, scirust-probability, scirust-inference, scirust-genetic, scirust-quantum) are WASM-compatible — they use only core/std with no network or filesystem requirements.

To build for WASM:

```bash
# Install target (requires network)
rustup target add wasm32-unknown-unknown

# Build (skip CLI and distributed which use std::net/std::process)
cargo build -p scirust-inference -p scirust-probability -p scirust-quantum -p scirust-genetic --target wasm32-unknown-unknown
```

**Crates that won't compile to WASM:**
- `scirust-cli` — uses std::process
- `scirust-distributed` — uses std::net, std::thread
- `scirust-rustc-driver` — nightly Rust only

**Note:** `scirust-simd` uses x86_64 SIMD intrinsics but falls back to scalar on non-x86 targets (WASM included).

