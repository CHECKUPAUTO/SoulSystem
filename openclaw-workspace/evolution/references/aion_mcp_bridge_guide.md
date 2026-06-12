# AION MCP Bridge Guide

**Purpose:** Document the MCP server pattern for AION (Large Omnimodal Model for Astronomy) integration, enabling OpenClaw to treat astronomical data as a first-class modality.

**Source:** OpenEvolve Night Cycle 2026-04-12 (AION + Lobster integration analysis)

## Overview

AION is a specialized Large Omnimodal Model designed for astronomy. This guide describes how to bridge AION's capabilities into OpenClaw's tool ecosystem via an MCP (Model Context Protocol) server.

## Core Capabilities

### AION Features
- **Multimodal Understanding**: Processes images, spectra, time-series, and catalog data
- **Scientific Reasoning**: Domain-specific knowledge for astronomical analysis
- **Cross-Modal Queries**: "Find galaxies similar to this image in the spectroscopic catalog"

### Integration Benefits
- Treat astronomical data as first-class modality (similar to image/audio)
- Enable scientific workflows in OpenClaw
- Access The Well dataset and other astronomical repositories

## MCP Server Architecture

### Server Structure
```typescript
// mcp-servers/aion-bridge/src/server.ts
import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';

const server = new Server({
  name: 'aion-astronomy-bridge',
  version: '1.0.0'
}, {
  capabilities: {
    tools: {
      'aion/analyze-image': analyzeAstronomicalImage,
      'aion/query-catalog': queryAstronomicalCatalog,
      'aion/compare-spectra': compareSpectra,
      'aion/generate-cutout': generateImageCutout
    },
    resources: {
      'aion://datasets/{datasetId}': getDatasetInfo,
      'aion://observations/{obsId}': getObservationData
    }
  }
});
```

### Tool Definitions

#### analyzeAstronomicalImage
```typescript
{
  name: 'aion/analyze-image',
  description: 'Analyze astronomical image using AION',
  inputSchema: {
    type: 'object',
    properties: {
      image: { type: 'string', format: 'binary' },
      analysisType: {
        type: 'string',
        enum: ['object-detection', 'classification', 'anomaly-detection']
      },
      catalog: { type: 'string', description: 'Cross-match catalog' }
    },
    required: ['image', 'analysisType']
  }
}
```

#### queryAstronomicalCatalog
```typescript
{
  name: 'aion/query-catalog',
  description: 'Query astronomical catalog with natural language',
  inputSchema: {
    type: 'object',
    properties: {
      query: { type: 'string' },
      catalog: {
        type: 'string',
        enum: ['sdss', 'desi', 'gaia', 'wise']
      },
      constraints: {
        type: 'object',
        properties: {
          ra: { type: 'number' },
          dec: { type: 'number' },
          radius: { type: 'number' },
          magnitude: { type: 'object' }
        }
      }
    },
    required: ['query']
  }
}
```

## Implementation Guide

### Step 1: AION Client Setup
```typescript
// src/aion/client.ts
import { AIONClient } from '@polymathic/aion-sdk';

export class AIONBridgeClient {
  private client: AIONClient;
  
  constructor(config: AIONConfig) {
    this.client = new AIONClient({
      endpoint: config.aionEndpoint,
      apiKey: config.apiKey,
      cacheDir: config.cacheDir
    });
  }
  
  async analyzeImage(image: Buffer, options: AnalysisOptions) {
    return this.client.analyze({
      image,
      modality: 'astronomy',
      task: options.analysisType,
      catalog: options.catalog
    });
  }
  
  async queryCatalog(query: string, constraints: QueryConstraints) {
    return this.client.query({
      naturalLanguage: query,
      ...constraints
    });
  }
}
```

### Step 2: Tool Handlers
```typescript
// src/handlers/tools.ts
export const toolHandlers = {
  'aion/analyze-image': async (args) => {
    const image = await loadImage(args.image);
    const result = await aionClient.analyzeImage(image, {
      analysisType: args.analysisType,
      catalog: args.catalog
    });
    
    return {
      content: [{
        type: 'text',
        text: formatAIONResult(result)
      }],
      annotations: {
        confidence: result.confidence,
        sources: result.crossMatches
      }
    };
  },
  
  'aion/query-catalog': async (args) => {
    const results = await aionClient.queryCatalog(args.query, args.constraints);
    
    return {
      content: [{
        type: 'text',
        text: formatCatalogResults(results)
      }],
      data: results.objects // Structured data for downstream tools
    };
  }
};
```

### Step 3: Resource Providers
```typescript
// src/handlers/resources.ts
export const resourceHandlers = {
  'aion://datasets/{datasetId}': async (uri, params) => {
    const dataset = await aionClient.getDataset(params.datasetId);
    
    return {
      contents: [{
        uri: uri.href,
        mimeType: 'application/json',
        text: JSON.stringify(dataset.metadata)
      }]
    };
  }
};
```

## Skill Integration

### AION Skill for OpenClaw
```typescript
// skills/aion-bridge/SKILL.md
export const aionSkill = {
  name: 'aion-astronomy',
  description: 'Access astronomical data analysis via AION',
  
  tools: ['aion/analyze-image', 'aion/query-catalog'],
  
  examples: [
    {
      prompt: 'Analyze this telescope image for galaxies',
      workflow: [
        { tool: 'aion/analyze-image', args: { analysisType: 'object-detection' } }
      ]
    },
    {
      prompt: 'Find stars brighter than magnitude 15 near RA 180 Dec +30',
      workflow: [
        { 
          tool: 'aion/query-catalog', 
          args: { 
            query: 'stars brighter than magnitude 15',
            constraints: { ra: 180, dec: 30, radius: 1 }
          } 
        }
      ]
    }
  ]
};
```

## Data Flow

### Image Analysis Pipeline
```
User uploads image → OpenClaw → AION MCP Server → AION API → Analysis Result
                                           ↓
                                    Cross-match with catalogs
                                           ↓
                                    Return structured findings
```

### Catalog Query Pipeline
```
Natural language query → OpenClaw → AION MCP Server → AION Query Engine
                                                        ↓
                                              Convert to SQL/catalog API
                                                        ↓
                                              Return matching objects
```

## Configuration

### OpenClaw Gateway Config
```yaml
# config/mcp-servers.yaml
mcpServers:
  aion-bridge:
    command: node
    args: ['./mcp-servers/aion-bridge/dist/server.js']
    env:
      AION_ENDPOINT: ${AION_API_URL}
      AION_API_KEY: ${AION_API_KEY}
      CACHE_DIR: /var/cache/aion
```

### Environment Variables
```bash
# .env
AION_API_URL=https://api.aion.polymathic.ai
AION_API_KEY=your_api_key_here
AION_CACHE_TTL=3600
```

## Use Cases

### Research Assistant
- "Analyze this FITS file for exoplanet candidates"
- "Compare these two spectra and identify differences"
- "Find all galaxies in this image that match the training set"

### Data Discovery
- "Show me observations from DESI in this region"
- "What datasets contain information about quasars at z>6?"
- "Cross-match this catalog with Gaia DR3"

### Visualization
- "Generate a color composite from these three filters"
- "Create a light curve for this variable star"
- "Plot the redshift distribution of these galaxies"

## Performance Considerations

- **Caching**: Cache AION responses for common queries
- **Streaming**: Support streaming for large catalog results
- **Pagination**: Handle large result sets via pagination
- **Rate Limiting**: Respect AION API rate limits

## Error Handling

```typescript
// src/errors.ts
class AIONError extends Error {
  constructor(
    message: string,
    public code: string,
    public retryable: boolean
  ) {
    super(message);
  }
}

// Map AION errors to MCP error codes
const errorMap = {
  'RATE_LIMIT': { code: -32000, retryable: true },
  'INVALID_IMAGE': { code: -32602, retryable: false },
  'CATALOG_UNAVAILABLE': { code: -32001, retryable: true }
};
```

## Related Patterns

- `polymathic_integration_patterns.md` - General PolymathicAI integration
- `lobster_workflow_integration_guide.md` - Scientific workflow orchestration
- `mcp_gateway_integration_guide.md` - MCP server implementation patterns

## Action Items

- [ ] Create MCP server scaffolding for AION bridge
- [ ] Implement core tool handlers (analyze-image, query-catalog)
- [ ] Add caching layer for common queries
- [ ] Create OpenClaw skill wrapper
- [ ] Document example workflows for astronomy research
