# Neural Store

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
![Rust](https://img.shields.io/badge/language-Rust-orange.svg)

**Neural Store** is a high-performance vector database and persistent memory system designed for AI agents and machine learning applications. It combines a Log-Structured Merge Tree (LSM-Tree) architecture with SIMD-accelerated similarity search to provide fast, durable, and scalable storage for high-dimensional embeddings.

---

## 🚀 Key Features

- **LSM-Tree Storage Engine**: Optimized for high-throughput writes and efficient memory management.
- **Durable WAL**: Write-Ahead Logging ensures data integrity and recovery after crashes.
- **SIMD-Accelerated Search**: Hardware-optimized implementations (AVX2, AVX-512, Neon) for Cosine and Mahalanobis similarity.
- **Multi-threaded Querying**: Parallel search powered by [Rayon](https://github.com/rayon-rs/rayon).
- **Background "Brain" Workers**: Automated garbage collection and vector clustering to maintain performance over time.
- **C/C++ FFI**: Seamless integration with other languages via a stable C interface.

---

## 🛠 Prerequisites

- **Rust**: Stable toolchain (1.70+ recommended).
- **Hardware**: x86_64 (with AVX2/AVX-512) or AArch64 (Neon) for optimal performance.

---

## 📦 Installation

Add Neural Store to your `Cargo.toml`:

```toml
[dependencies]
neural_store = { git = "https://github.com/CHECKUPAUTO/neural_store" }
```

Or clone and build from source:

```bash
git clone https://github.com/CHECKUPAUTO/neural_store.git
cd neural_store
cargo build --release
```

---

## 💡 Usage

### Rust API

```rust
use neural_store::{NeuralStore, Vector};

fn main() -> anyhow::Result<()> {
    // Open or create a store
    let mut store = NeuralStore::open("my_vectors")?;

    // Insert a vector
    let v = Vector::new(vec![1.0, 0.0, 0.0, 0.0]);
    store.put(1, v)?;

    // Search for top 5 similar vectors
    let query = vec![0.9, 0.1, 0.0, 0.0];
    let results = store.search(&query, 5);

    for (id, score) in results {
        println!("Found match: ID={}, Similarity={}", id, score);
    }

    Ok(())
}
```

### C FFI

Neural Store provides C-compatible bindings in `src/ffi`.

```c
#include "neural_store.h"

int main() {
    ns_init();

    float vec[] = {1.0, 0.0, 0.0, 0.0};
    ns_put(1, vec, 4);

    size_t count;
    SearchResult* results = ns_search(vec, 4, 5, &count);

    // Process results...

    ns_free(results, count);
    return 0;
}
```

---

## 🏗 Architecture

Neural Store is composed of several specialized modules:

```text
neural_store/
├── src/
│   ├── core/         # Search engine, SIMD kernels, and distance metrics
│   ├── storage/      # LSM-Tree, MemTable, and WAL (Persistence)
│   ├── brain/        # Background workers (GC, Clustering)
│   ├── ffi/          # C-compatible bindings
│   └── lib.rs        # Main entry point & orchestrator
└── tests/            # Integration and benchmark tests
```

### Data Flow
1. **Write**: `put()` -> WAL (Disk) -> MemTable (In-memory SkipMap).
2. **Search**: Query -> SearchEngine -> Parallel SIMD Scan -> Top-K results.
3. **Maintenance**: `BrainWorkers` periodically run GC and clustering in the background.

---

## ⚙️ Configuration

The store can be configured via environment variables (planned) or by modifying `src/core/types.rs` constants:

- `DEFAULT_DIMENSION`: 128
- `MAX_SEGMENT_SIZE`: 65,536 entries
- `WAL_INITIAL_SIZE`: 64 MB

---

## 🤝 Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for details on our code of conduct and the process for submitting pull requests.

---

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

## 🙏 Acknowledgments

- Inspired by modern vector databases like Qdrant and Milvus.
- Built with [Rayon](https://github.com/rayon-rs/rayon) and [Crossbeam](https://github.com/crossbeam-rs/crossbeam).
