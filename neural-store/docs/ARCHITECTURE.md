# Neural Store Architecture

Neural Store is built with a focus on high-performance writes and extremely fast vector similarity searches. This document details the core architectural components.

## High-Level Overview

Neural Store follows a modular architecture where storage, search, and background maintenance are decoupled.

```text
[ API / FFI ]
      |
[ NeuralStore Orchestrator ]
    /          \
[ LsmTree ]  [ SearchEngine ]
   |            |
[ WAL ]      [ SIMD Kernels ]
[ MemTable ] [ Distance Metrics ]
```

## 1. Storage Engine (LSM-Tree)

To handle high-throughput vector insertions, Neural Store uses a **Log-Structured Merge Tree (LSM-Tree)**.

### Write-Ahead Log (WAL)
Every `put` operation is first appended to a Write-Ahead Log. This ensures that even if the system crashes before data is flushed to permanent segments, it can be recovered. The WAL is implemented using memory-mapped files (`memmap2`) for low-latency I/O.

### MemTable
After being logged, data is inserted into the `MemTable`. We use a lock-free **SkipMap** (from `crossbeam-skiplist`) to allow high concurrency during writes and point lookups.

### Persistence & Recovery
On startup, the `LsmTree` scans the WAL and replays all entries to rebuild the `MemTable`.

## 2. Search Engine

The search engine is designed for parallel, hardware-accelerated similarity scanning.

### SIMD Acceleration
Neural Store uses dynamic dispatch to select the best SIMD (Single Instruction, Multiple Data) backend at runtime:
- **AVX-512 / AVX2**: For modern x86_64 CPUs.
- **Neon**: For ARM64 (Apple Silicon, AWS Graviton).
- **Scalar**: Fallback for unsupported architectures.

These kernels accelerate the dot product and subtraction operations which are the bottlenecks in vector distance calculation.

### Parallel Execution
Using **Rayon**, search queries are distributed across all available CPU cores. The dataset is partitioned into chunks, and each core processes a chunk using SIMD instructions.

## 3. Brain Workers

A "Neural" store requires background maintenance to optimize its internal representation. The `BrainWorkers` handle these tasks:

- **Garbage Collection (GC)**: Reclaims space from overwritten or deleted vectors.
- **Clustering**: (WIP) Periodically re-organizes vectors into clusters to enable Approximate Nearest Neighbor (ANN) search in the future.

## 4. Distance Metrics

Supported metrics include:
- **Cosine Similarity**: Measures the cosine of the angle between two vectors. Ideal for normalized embeddings.
- **Mahalanobis Distance**: Accounts for correlations between variables and is useful for non-spherical data distributions.
- **L2 (Euclidean)** & **Inner Product**: Standard metrics for various ML models.
