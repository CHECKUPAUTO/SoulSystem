# Performance Optimization Proposals (2026-04-14)

> Extracted from night_cycle_20260414_0000.md — Section 5.3

## Proposals

### 1. SIMD Vector Search
- **Current**: float32 embeddings, no hardware acceleration
- **Proposal**: Replace with std::simd + quantized vectors (8-bit = 4x memory reduction)
- **Impact**: Significant search speedup + memory savings
- **Auto-apply**: ❌ Core code — requires Rust implementation

### 2. Lock-Free Inter-Organ Communication
- **Current**: HTTP between organs (overhead for co-located services)
- **Proposal**: Replace HTTP with shared-memory channels (crossbeam) for co-located organs
- **Impact**: Sub-millisecond inter-organ latency
- **Auto-apply**: ❌ Core code — requires Rust implementation

### 3. Batch Consolidation
- **Current**: Per-insert memory consolidation
- **Proposal**: Memory organ should batch consolidation cycles (every 15min) instead of per-insert
- **Impact**: Reduced I/O pressure, better batching
- **Auto-apply**: ❌ Core code — requires organ implementation

### 4. Connection Pooling
- **Current**: Per-request HTTP connections between organs
- **Proposal**: Mesh bridge should use persistent HTTP/2 connections between organs
- **Impact**: Reduced connection overhead, lower latency
- **Auto-apply**: ❌ Core code — requires mesh bridge changes

## Additional Proposals (from 02:00 cycle)

### 5. Connection Pooling (Persistent HTTP/2)
- **Current**: Per-request HTTP connections between brain nodes
- **Proposal**: Persistent HTTP/2 connections between organs
- **Impact**: 30-50% latency reduction
- **Effort**: Low
- **Auto-apply**: ❌ Core code — requires mesh bridge changes

### 6. Batch Stimuli Processing
- **Current**: Individual stimulus processing per request
- **Proposal**: Group stimuli during low-turbulence periods for batch processing
- **Impact**: 10x throughput in idle periods
- **Effort**: Medium
- **Auto-apply**: ❌ Core code — requires orchestrator changes

### 7. Zero-Copy Deserialization
- **Current**: Full JSON deserialization overhead
- **Proposal**: Use `serde_json::from_slice` with borrowed data where possible
- **Impact**: 15-20% parsing speedup
- **Effort**: Low
- **Auto-apply**: ❌ Core code — requires Rust implementation

### 8. Lock-Free State (DashMap)
- **Current**: `Arc<Mutex<>>` in soullink-node hot paths
- **Proposal**: Replace with `DashMap` for lock-free concurrent access
- **Impact**: Eliminate contention in hot paths
- **Effort**: Low
- **Auto-apply**: ❌ Core code — requires Rust implementation

### 9. Binary Inter-Node Protocol
- **Current**: JSON serialization between brain nodes
- **Proposal**: Replace JSON with bincode for 3-5x serialization speedup
- **Impact**: 3-5x inter-node communication speedup
- **Effort**: Medium
- **Auto-apply**: ❌ Core code — requires Rust implementation

## Source

- `night_cycle_20260414_0000.md`
- `night_cycle_20260414_0200.md`
- `night_cycle_20260414_0230.md`

## Last Updated

2026-04-14T02:38:00+02:00 — Auto-apply cycle (added 02:30 proposals: node binary unification, RocksDB shared instance, evaluator merge)

### 10. Node Binary Unification (from 02:30)
- **Current**: 6 separate soullink-node processes (~55M RAM total)
- **Proposal**: Single launcher managing all 6 as threads instead of processes
- **Impact**: ~55M RAM savings (6 × 9M), reduced IPC overhead
- **Auto-apply**: ❌ Core code — requires Rust implementation

### 11. RocksDB Shared Instance (from 02:30)
- **Current**: Each node has its own RocksDB instance
- **Proposal**: Share a single RocksDB instance with column families per organ
- **Impact**: Reduced disk seeks, enables cross-organ queries
- **Auto-apply**: ❌ Core code — requires Rust implementation

### 12. Evaluator Binary Merge (from 02:30)
- **Current**: Separate evaluator binary at `/root/.openclaw/workspace/soullink-node/evaluator/`
- **Proposal**: Merge evaluator into main soullink-node build
- **Impact**: Reduced binary count, simpler deployment
- **Auto-apply**: ❌ Core code — requires Rust build system changes