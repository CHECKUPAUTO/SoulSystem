# Dream Quality Metric Proposal

**Created:** 2026-04-13 (Night Cycle 00:30)
**Source:** OpenEvolve Night Cycle Report 2026-04-13 00:30
**Status:** Proposal
**Priority:** P3

## Context

The Dreaming UI has matured with:
- Advanced review tab for memory curation
- Diary navigation with phase labels
- Unknown phase state preservation
- i18n for phase labels
- Waiting queue sort by recency

The phase-aware UI (waiting → processing → reviewing → integrating) mirrors cognitive science stages of memory consolidation.

## Proposal: Self-Assessment Dream Quality Score

Add a simple quality metric (1-5) per dreaming session stored in the memory wiki:

```typescript
interface DreamQualityMetric {
  sessionId: string;
  score: 1 | 2 | 3 | 4 | 5;  // Self-assessment
  phases: {
    waiting: number;      // ms spent in each phase
    processing: number;
    reviewing: number;
    integrating: number;
  };
  memoriesCreated: number;
  memoriesUpdated: number;
  memoriesDiscarded: number;
  timestamp: string;
}
```

### Scoring Guide

| Score | Meaning |
|-------|---------|
| 1 | No useful output, noise only |
| 2 | Minor insights, mostly redundant |
| 3 | Average session, some useful consolidation |
| 4 | Good session, meaningful new connections |
| 5 | Breakthrough, novel insights or corrections |

### Feedback Loop

Over time, correlate dream quality with:
- Phase durations (optimal time in each phase)
- Memory source selection (which sources yield better dreams)
- Turbulence/attractor state (neural dynamics during dreaming)

This creates a data-driven approach to tuning dreaming parameters.

## References

- Dreaming LTM architecture: `evolution/references/dreaming_ltm_architecture.md`
- Dreaming UI commits: `64693d2e96`, `f479ab1498`-`cc387edf87`, `279cbfc61c`, `03f19c5abe1`-`0202af9b38`