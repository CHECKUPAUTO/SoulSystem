# Pure Function Extraction for Graphify

**Priority:** P1 (HIGH)
**Source:** Night Cycle 2026-04-13 01:15
**Status:** Proposal

## Problem

The `graphify` watch module's `_rebuild_code` function mixes pure AST diff computation with impure file I/O and watch triggers. This makes it difficult to test graph updates without a filesystem and slows CI.

## Proposal

Split `_rebuild_code` into:

1. **Pure: `computeGraphDiff(oldAST, newAST)`** — Pure function that computes the diff between two AST snapshots. No I/O, no side effects. Fully testable in isolation.

2. **Pure: `applyDiffToGraph(diff, graph)`** — Pure function that applies a computed diff to the graph data structure. Returns new graph.

3. **Impure: `watchAndRebuild(path, onChange)`** — Thin wrapper that handles file watching, reads files, calls pure functions, and triggers side effects.

## Benefits

- **Testability:** Graph updates can be tested without filesystem mocking
- **CI Speed:** Pure functions are orders of magnitude faster to test than I/O-bound watchers
- **Debuggability:** Diffs can be inspected independently of watch state
- **Composability:** Pure diff computation can be reused for other purposes (e.g., incremental graph updates over VCS history)

## Implementation Pattern

```python
# Before (impure)
def _rebuild_code(path):
    files = read_all_files(path)  # I/O
    ast = parse_files(files)       # I/O
    diff = compute_diff(old_ast, ast)
    apply_to_graph(diff)           # Side effect
    write_report(diff)             # I/O

# After (pure + impure)
def compute_graph_diff(old_ast: AST, new_ast: AST) -> GraphDiff:
    """Pure function - fully testable"""
    ...

def apply_diff_to_graph(diff: GraphDiff, graph: Graph) -> Graph:
    """Pure function - fully testable"""
    ...

def watch_and_rebuild(path: Path, on_change: Callable):
    """Impure wrapper - thin, minimal logic"""
    files = read_all_files(path)
    new_ast = parse_files(files)
    diff = compute_graph_diff(current_ast, new_ast)
    graph = apply_diff_to_graph(diff, current_graph)
    write_report(diff)
```

## Related References

- `explicit_seams_pattern.md` — Same principle applied to module boundaries
- `pure_test_migration_pattern.md` — Pattern for migrating tests to pure coverage
- `barrel_bypassing_guide.md` — Related import optimization

## Cross-References

- IronReview T430 analysis identified this as a ★★★★★ pattern opportunity
- Aligns with OpenClaw's systematic pure-test extraction campaign (14+ commits in 3 days)