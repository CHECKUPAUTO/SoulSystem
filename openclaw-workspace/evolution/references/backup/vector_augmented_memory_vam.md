# Vector-Augmented Memory (VAM) Pattern

**Purpose:** Hybrid memory approach combining file-based recall with graphify-out structure for enhanced long-term memory retrieval.

**Source:** OpenEvolve Night Cycle 2026-04-12 (Dreaming/LTM Architecture analysis)

## Overview

Vector-Augmented Memory (VAM) moves from simple file-based recall to a hybrid approach using the existing `graphify-out` structure. It enables semantic similarity search while maintaining human-readable persistence.

## Current State

### Simple File-Based Recall
```
memory/
├── 2026-04-10.md
├── 2026-04-11.md
├── 2026-04-12.md
└── archive.md
```

**Limitations:**
- Linear search through files
- Keyword-based matching only
- No semantic understanding
- Cannot find "similar but not identical" memories

## VAM Architecture

### Hybrid Structure
```
memory/
├── raw/                          # Human-readable daily notes
│   ├── 2026-04-10.md
│   ├── 2026-04-11.md
│   └── 2026-04-12.md
├── vectors/                      # Vector embeddings
│   ├── 2026-04-10.json
│   ├── 2026-04-11.json
│   └── 2026-04-12.json
├── index/                        # Search indices
│   ├── semantic-index.json
│   └── temporal-index.json
└── graphify/                     # Graph relationships
    ├── entities.json
    ├── relationships.json
    └── communities.json
```

## Implementation

### Step 1: Memory Chunking
```typescript
// src/memory/chunker.ts
interface MemoryChunk {
  id: string;
  source: string;          // Original file
  timestamp: number;
  content: string;
  embedding?: number[];      // Vector embedding
  entities: string[];        // Extracted entities
  topics: string[];          // Categorized topics
  importance: number;        // 0-1 relevance score
}

export function chunkMemory(content: string, options: ChunkOptions): MemoryChunk[] {
  // Split by semantic boundaries (paragraphs, topics, decisions)
  const chunks = semanticSplit(content, {
    maxSize: 512,
    overlap: 64,
    preserveContext: true
  });
  
  return chunks.map((chunk, i) => ({
    id: generateId(),
    source: options.sourceFile,
    timestamp: options.timestamp,
    content: chunk.text,
    entities: extractEntities(chunk.text),
    topics: categorizeTopics(chunk.text),
    importance: scoreImportance(chunk.text)
  }));
}
```

### Step 2: Embedding Generation
```typescript
// src/memory/embedder.ts
export async function generateEmbeddings(chunks: MemoryChunk[]): Promise<MemoryChunk[]> {
  const model = await loadEmbeddingModel('text-embedding-3-small');
  
  // Batch embedding for efficiency
  const embeddings = await model.embedBatch(
    chunks.map(c => c.content)
  );
  
  return chunks.map((chunk, i) => ({
    ...chunk,
    embedding: embeddings[i]
  }));
}
```

### Step 3: Graph Integration
```typescript
// src/memory/graph-integration.ts
interface MemoryNode {
  id: string;
  type: 'memory' | 'entity' | 'topic' | 'decision';
  properties: {
    content?: string;
    timestamp: number;
    importance: number;
  };
}

interface MemoryEdge {
  source: string;
  target: string;
  type: 'mentions' | 'related_to' | 'decided_by' | 'temporal';
  weight: number;
}

export function buildMemoryGraph(chunks: MemoryChunk[]): { nodes: MemoryNode[], edges: MemoryEdge[] } {
  const nodes: MemoryNode[] = [];
  const edges: MemoryEdge[] = [];
  
  for (const chunk of chunks) {
    // Memory node
    nodes.push({
      id: chunk.id,
      type: 'memory',
      properties: {
        content: chunk.content,
        timestamp: chunk.timestamp,
        importance: chunk.importance
      }
    });
    
    // Entity nodes and edges
    for (const entity of chunk.entities) {
      const entityId = `entity:${entity}`;
      if (!nodes.find(n => n.id === entityId)) {
        nodes.push({
          id: entityId,
          type: 'entity',
          properties: { timestamp: chunk.timestamp, importance: 0.8 }
        });
      }
      edges.push({
        source: chunk.id,
        target: entityId,
        type: 'mentions',
        weight: 0.9
      });
    }
    
    // Topic nodes
    for (const topic of chunk.topics) {
      const topicId = `topic:${topic}`;
      if (!nodes.find(n => n.id === topicId)) {
        nodes.push({
          id: topicId,
          type: 'topic',
          properties: { timestamp: chunk.timestamp, importance: 0.7 }
        });
      }
      edges.push({
        source: chunk.id,
        target: topicId,
        type: 'related_to',
        weight: 0.8
      });
    }
  }
  
  return { nodes, edges };
}
```

## Cognitive Regimes

### Neural State Integration

Integrate V12 Neural State (Turbulence/Attractors) into memory weighting:

```typescript
// src/memory/cognitive-regimes.ts
interface NeuralState {
  turbulence: number;
  attractor: 'DeepBasin' | 'StableOrbit' | 'StrangeAttractor' | 'Transient';
  scienceActivation: number;
  engineerActivation: number;
  creativeActivation: number;
}

export function applyCognitiveRegime(
  chunks: MemoryChunk[],
  neuralState: NeuralState
): MemoryChunk[] {
  if (neuralState.turbulence > 0.1) {
    // High turbulence: favor creative/strange associations
    return chunks.map(c => ({
      ...c,
      // Boost creative topics
      relevance: calculateCreativeRelevance(c, neuralState)
    }));
  } else {
    // Low turbulence: favor exact matches and analytical recall
    return chunks.map(c => ({
      ...c,
      // Boost science/engineer topics
      relevance: calculateAnalyticalRelevance(c, neuralState)
    }));
  }
}

function calculateCreativeRelevance(chunk: MemoryChunk, state: NeuralState): number {
  const creativeBoost = chunk.topics.some(t => 
    ['creative', 'novel', 'explore', 'brainstorm'].includes(t)
  ) ? 0.3 : 0;
  
  const strangeAssociation = chunk.entities.length > 3 ? 0.2 : 0;
  
  return chunk.importance + creativeBoost + strangeAssociation;
}

function calculateAnalyticalRelevance(chunk: MemoryChunk, state: NeuralState): number {
  const analyticalBoost = chunk.topics.some(t =>
    ['analysis', 'code', 'architecture', 'refactor'].includes(t)
  ) ? 0.3 : 0;
  
  return chunk.importance + analyticalBoost;
}
```

## Retrieval Strategies

### Semantic Search
```typescript
// src/memory/retrieval.ts
export async function semanticSearch(
  query: string,
  options: SearchOptions
): Promise<MemoryChunk[]> {
  const queryEmbedding = await generateEmbedding(query);
  
  // Find similar vectors
  const candidates = await vectorIndex.search(queryEmbedding, {
    topK: options.limit * 2,
    minSimilarity: 0.7
  });
  
  // Apply neural state weighting
  const neuralState = await getCurrentNeuralState();
  const weighted = applyCognitiveRegime(candidates, neuralState);
  
  // Rerank by graph importance
  const reranked = rerankByGraphCentrality(weighted);
  
  return reranked.slice(0, options.limit);
}
```

### Temporal + Semantic Hybrid
```typescript
export async function hybridSearch(
  query: string,
  timeWindow: TimeRange,
  options: SearchOptions
): Promise<MemoryChunk[]> {
  // Get recent memories within time window
  const recent = await temporalIndex.getRange(timeWindow);
  
  // Get semantically similar memories (any time)
  const similar = await semanticSearch(query, { limit: 20 });
  
  // Combine and deduplicate
  const combined = [...recent, ...similar];
  const unique = deduplicateById(combined);
  
  // Score by temporal proximity + semantic similarity
  return unique
    .map(m => ({
      ...m,
      score: combineScores(
        temporalScore(m.timestamp, timeWindow),
        semanticScore(m.embedding, query)
      )
    }))
    .sort((a, b) => b.score - a.score)
    .slice(0, options.limit);
}
```

## Integration with Active Memory

```typescript
// src/memory/active-memory.ts
export class ActiveMemory {
  private vam: VectorAugmentedMemory;
  
  async preloadForSession(sessionId: string): Promise<void> {
    // Load recent context
    const recent = await this.vam.getRecent({ days: 7 });
    
    // Get current neural state
    const neuralState = await this.getNeuralState();
    
    // Apply cognitive regime
    const relevant = applyCognitiveRegime(recent, neuralState);
    
    // Preload into session context
    await this.context.load(relevant);
  }
  
  async searchContext(query: string): Promise<MemoryChunk[]> {
    return this.vam.search(query, {
      includeGraph: true,
      applyCognitiveRegime: true
    });
  }
}
```

## File Format

### Vector Storage (JSONL)
```jsonl
// memory/vectors/2026-04-12.jsonl
{"id":"mem-001","timestamp":1744413600,"content":"Discussed OpenClaw security hardening...","embedding":[0.023,-0.156,...],"entities":["OpenClaw","security"],"topics":["security","architecture"],"importance":0.85}
{"id":"mem-002","timestamp":1744413700,"content":"Implemented VAM pattern...","embedding":[0.045,-0.089,...],"entities":["VAM","memory"],"topics":["architecture","memory"],"importance":0.9}
```

## Benefits

1. **Semantic Understanding**: Find related concepts even with different wording
2. **Cognitive Alignment**: Memory retrieval adapts to current neural state
3. **Graph Relationships**: Discover non-obvious connections between memories
4. **Human Readable**: Original files remain editable and version-controlled
5. **Incremental**: Can be adopted gradually alongside existing memory system

## Migration Path

1. **Phase 1**: Continue file-based memory, add vector index in background
2. **Phase 2**: Enable semantic search alongside keyword search
3. **Phase 3**: Full integration with neural state weighting
4. **Phase 4**: Graph-based discovery and recommendation

## Related Patterns

- `dreaming_ltm_architecture_guide.md` - Long-term memory architecture
- `session_state_management_patterns.md` - Session state persistence
- `graphify-out/GRAPH_REPORT.md` - Graph structure analysis

## Action Items

- [ ] Implement memory chunking strategy
- [ ] Add embedding generation pipeline
- [ ] Build graph integration layer
- [ ] Create cognitive regime scoring
- [ ] Integrate with existing graphify-out structure
