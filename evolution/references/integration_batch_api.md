# Batch API for Integration Organ

**Source:** OpenEvolve Night Cycle 131 (2026-04-14 14:02)  
**Priority:** MEDIUM — throughput optimization  
**Status:** Proposal (requires Integration organ implementation)

## Current State

Integration organ processes requests one at a time. When multiple organs need to contribute to a synthesis (e.g., Science + Engineer + Reasoning), each request is handled sequentially, adding round-trip latency per organ.

## Proposal

Add a Batch API to the Integration organ that groups multiple cross-organ requests into a single round:

### API Design

```
POST /api/integration/batch-synthesize
{
  "inputs": [
    {"organ": "science", "query": "verify_hypothesis", "weight": 0.3},
    {"organ": "engineer", "query": "implementation_plan", "weight": 0.4},
    {"organ": "reasoning", "query": "logical_consistency", "weight": 0.3}
  ],
  "strategy": "weighted_merge",
  "timeout_ms": 200
}
```

### Architecture

```rust
// Parallel fan-out to multiple organs, fan-in synthesis
async fn batch_synthesize(req: BatchRequest) -> SynthesisResult {
    let futures: Vec<_> = req.inputs.iter()
        .map(|input| organ_client.query(input.organ, input.query))
        .collect();
    
    let results = tokio::time::timeout(
        Duration::from_millis(req.timeout_ms),
        futures::future::join_all(futures)
    ).await?;
    
    synthesize(results, req.strategy, req.weights)
}
```

### Expected Benefits

- **10x throughput** for multi-organ synthesis (parallel vs sequential)
- **Lower tail latency** — bounded by slowest organ, not sum of all
- **Timeout handling** — partial results if one organ is slow
- **Weight-based synthesis** — importance-weighted combination

### Dependencies

- Integration organ (port 9036) must be implemented
- Organ clients need async HTTP support
- Timeout and partial-result handling in synthesis engine