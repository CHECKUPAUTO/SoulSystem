# Memory Search Quality Metrics

**Priority:** Low (from 0230 report, S4)  
**Status:** Proposal  
**Created:** 2026-04-13  
**Source:** OpenEvolve Night Cycle 0230

## Problem

Memory search quality improvements (`improve memory fallback lexical ranking`) and Unicode slug fixes are landing, but there's no benchmark to catch regressions.

## Proposal: Recall@K Benchmark

```python
# benchmark/memory_search.py
class MemorySearchBenchmark:
    """Benchmark recall@k for memory search quality."""
    
    def benchmark_lexical(self, queries: list[dict]) -> dict:
        """Test lexical fallback search quality."""
        results = {'recall@1': 0, 'recall@3': 0, 'recall@5': 0}
        for query in queries:
            hits = lexical_search(query['text'], top_k=5)
            for k, metric in [(1, 'recall@1'), (3, 'recall@3'), (5, 'recall@5')]:
                if query['expected'] in hits[:k]:
                    results[metric] += 1
        return {k: v / len(queries) for k, v in results.items()}
```

## Key Metrics

- **Recall@1** — Is the expected result the top hit?
- **Recall@3** — Is the expected result in top 3?
- **Recall@5** — Is the expected result in top 5?
- **Unicode handling** — Do queries with non-ASCII characters work?
- **Latency** — Average search time per query

## Benefits

- Catch search quality regressions early
- Quantifiable improvement tracking for lexical ranking changes
- Unicode edge case coverage

## Related References

- `dreaming_ltm_architecture.md` — Long-term memory architecture