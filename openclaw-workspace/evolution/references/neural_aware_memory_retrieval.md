# Neural-Aware Memory Retrieval (VAM)

**Status:** Reference Documentation  
**Source:** night_cycle_20260412_0233.md  
**Priority:** P2 - Enhancement  
**Auto-Apply:** ❌ NO - Requires Neural State Integration  

## Overview

Vector-Augmented Memory (VAM) combines file-based persistence with vector semantic search and graphify-out structure. This guide proposes integrating the V12 Neural State (Turbulence/Attractors) into memory weighting.

## Problem Statement

- Current memory retrieval uses naive graph traversal
- No semantic similarity matching
- Memory weighting is static, not context-aware
- High-turbulence states need "creative associations", low-turbulence needs "exact matches"

## Solution: Cognitive-Regime-Aware Memory

### Architecture

```typescript
// src/memory/vam/vector-memory.ts
export interface VAMConfig {
  embeddingModel: string;           // e.g., 'sentence-transformers/all-MiniLM-L6-v2'
  vectorIndexPath: string;         // Path to HNSW or Faiss index
  dimension: number;               // Embedding dimension (384 for MiniLM)
  topK: number;                    // Default retrieval count
  similarityThreshold: number;     // Minimum cosine similarity
}

interface MemoryEntry {
  id: string;
  content: string;
  embedding: number[];
  metadata: MemoryMetadata;
  neuralWeights: NeuralWeights;
  createdAt: number;
  recallCount: number;
}

interface NeuralWeights {
  turbulenceAtCreation: number;     // Turbulence when memory was created
  attractorAtCreation: string;      // Attractor state at creation
  semanticWeight: number;          // How much to weight semantic similarity
  recencyWeight: number;           // How much to weight recency
  associationWeight: number;       // How much to weight "creative associations"
}

interface CognitiveRegime {
  turbulence: number;
  attractor: string;
  dominantNodes: Record<string, number>;
}
```

### Implementation

```typescript
import { HNSW } from 'hnswlib-node';
import { pipeline, PipelineType } from '@xenova/transformers';

export class VectorAugmentedMemory {
  private config: VAMConfig;
  private index: HNSW;
  private entries = new Map<string, MemoryEntry>();
  private embedder: any;

  constructor(config: Partial<VAMConfig> = {}) {
    this.config = {
      embeddingModel: 'sentence-transformers/all-MiniLM-L6-v2',
      vectorIndexPath: './.openclaw/memory/vectors',
      dimension: 384,
      topK: 10,
      similarityThreshold: 0.7,
      ...config
    };

    this.index = new HNSW('cosine', this.config.dimension);
  }

  async initialize(): Promise<void> {
    // Load embedding model
    this.embedder = await pipeline(
      'feature-extraction' as PipelineType,
      this.config.embeddingModel
    );

    // Load existing index or create new
    if (await this.indexExists()) {
      await this.index.loadIndex(this.config.vectorIndexPath);
    }
  }

  async addEntry(
    content: string,
    metadata: MemoryMetadata,
    neuralState: CognitiveRegime
  ): Promise<MemoryEntry> {
    const embedding = await this.generateEmbedding(content);
    
    const entry: MemoryEntry = {
      id: this.generateId(),
      content,
      embedding,
      metadata,
      neuralWeights: this.calculateNeuralWeights(neuralState),
      createdAt: Date.now(),
      recallCount: 0
    };

    // Add to index
    this.index.addPoint(embedding, this.entries.size);
    this.entries.set(entry.id, entry);

    return entry;
  }

  async query(
    query: string,
    currentNeuralState: CognitiveRegime,
    limit: number = this.config.topK
  ): Promise<ScoredMemory[]> {
    const queryEmbedding = await this.generateEmbedding(query);
    
    // Get candidates from vector index
    const candidates = this.index.searchKnn(
      queryEmbedding,
      limit * 3  // Get more candidates for re-ranking
    );

    // Score candidates with neural-aware weighting
    const scored = candidates.map(([entryId, distance]) => {
      const entry = this.entries.get(entryId);
      if (!entry) return null;

      const score = this.calculateNeuralAwareScore(
        entry,
        queryEmbedding,
        currentNeuralState
      );

      return { entry, score };
    }).filter(Boolean) as ScoredMemory[];

    // Sort by score and return top K
    return scored
      .sort((a, b) => b.score - a.score)
      .slice(0, limit);
  }

  private calculateNeuralAwareScore(
    entry: MemoryEntry,
    queryEmbedding: number[],
    currentState: CognitiveRegime
  ): number {
    // 1. Base semantic similarity
    const semanticScore = this.cosineSimilarity(
      entry.embedding,
      queryEmbedding
    );

    // 2. Neural regime matching
    const regimeScore = this.calculateRegimeMatch(
      entry.neuralWeights,
      currentState
    );

    // 3. Recency score (exponential decay)
    const age = Date.now() - entry.createdAt;
    const recencyScore = Math.exp(-age / (30 * 24 * 60 * 60 * 1000)); // 30-day half-life

    // 4. Usage score (frequently recalled memories)
    const usageScore = Math.log(entry.recallCount + 1) / Math.log(100);

    // Apply neural state weights
    if (currentState.turbulence > 0.1) {
      // High turbulence: favor creative associations
      return (
        semanticScore * 0.3 +
        regimeScore * 0.4 +
        usageScore * 0.2 +
        recencyScore * 0.1
      );
    } else {
      // Low turbulence: favor exact matches
      return (
        semanticScore * 0.5 +
        regimeScore * 0.2 +
        recencyScore * 0.2 +
        usageScore * 0.1
      );
    }
  }

  private calculateRegimeMatch(
    weights: NeuralWeights,
    currentState: CognitiveRegime
  ): number {
    // Similarity between memory's creation regime and current regime
    const turbulenceDiff = Math.abs(
      weights.turbulenceAtCreation - currentState.turbulence
    );
    
    const attractorMatch = weights.attractorAtCreation === currentState.attractor
      ? 1.0
      : 0.5;

    return (1 - turbulenceDiff) * attractorMatch;
  }

  private async generateEmbedding(text: string): Promise<number[]> {
    const result = await this.embedder(text, {
      pooling: 'mean',
      normalize: true
    });
    return result.data;
  }

  private cosineSimilarity(a: number[], b: number[]): number {
    const dotProduct = a.reduce((sum, val, i) => sum + val * b[i], 0);
    const magnitudeA = Math.sqrt(a.reduce((sum, val) => sum + val * val, 0));
    const magnitudeB = Math.sqrt(b.reduce((sum, val) => sum + val * val, 0));
    return dotProduct / (magnitudeA * magnitudeB);
  }

  private calculateNeuralWeights(state: CognitiveRegime): NeuralWeights {
    return {
      turbulenceAtCreation: state.turbulence,
      attractorAtCreation: state.attractor,
      semanticWeight: state.turbulence > 0.1 ? 0.3 : 0.5,
      recencyWeight: state.turbulence > 0.1 ? 0.1 : 0.2,
      associationWeight: state.turbulence > 0.1 ? 0.4 : 0.2
    };
  }

  private generateId(): string {
    return `mem_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }

  private async indexExists(): Promise<boolean> {
    try {
      await import('fs').then(fs => fs.promises.access(this.config.vectorIndexPath));
      return true;
    } catch {
      return false;
    }
  }
}

interface ScoredMemory {
  entry: MemoryEntry;
  score: number;
}
```

## Integration with Dreaming/LTM

```typescript
// src/memory/vam/dreaming-integration.ts
export class DreamingVAMIntegration {
  private vam: VectorAugmentedMemory;
  private graphifyOut: GraphifyOutClient;

  async consolidateMemory(): Promise<void> {
    // Get current neural state
    const neuralState = await this.fetchNeuralState();
    
    // Get recent entries from graphify-out
    const recentEntries = await this.graphifyOut.getRecentEntries(24); // Last 24h
    
    // Add to VAM with neural context
    for (const entry of recentEntries) {
      await this.vam.addEntry(
        entry.content,
        entry.metadata,
        neuralState
      );
    }

    // Run memory consolidation
    await this.consolidateSimilarMemories();
  }

  async queryWithContext(query: string): Promise<MemoryEntry[]> {
    const neuralState = await this.fetchNeuralState();
    return this.vam.query(query, neuralState);
  }

  private async fetchNeuralState(): Promise<CognitiveRegime> {
    const response = await fetch('http://127.0.0.1:9020/api/mesh/mind');
    const data = await response.json();
    
    return {
      turbulence: data.turbulence,
      attractor: data.attractor,
      dominantNodes: data.nodes || {}
    };
  }

  private async consolidateSimilarMemories(): Promise<void> {
    // Find semantically similar memories and strengthen connections
    // This is the "Deep" phase of dreaming
  }
}
```

## Cognitive Regimes

| Regime | Turbulence | Retrieval Strategy |
|--------|-----------|-------------------|
| **Analytical** | < 0.05 | Exact matches, recent items, high precision |
| **Stable** | 0.05-0.1 | Balanced: semantic + recency + usage |
| **Creative** | 0.1-0.2 | Loose associations, diverse recall, exploration |
| **Chaotic** | > 0.2 | Random sampling, distant associations, serendipity |

## Why Manual Implementation Required

This requires:
- Vector embedding model integration
- HNSW/Faiss vector index
- Neural state API integration
- Graphify-out API integration
- Index management and optimization
- Memory consolidation background tasks

## References

- Original: `night_cycle_20260412_0233.md`
- Dreaming Architecture: `dreaming_ltm_architecture.md`
- Graphify Integration: `graphify_out/`
