# Lobster Workflow Integration Guide

**Purpose:** Document the integration pattern for Lobster (deterministic workflow engine) with OpenClaw, enabling structured pipelines as Agent Skills.

**Source:** OpenEvolve Night Cycle 2026-04-12 (Lobster + AION integration analysis)

## Overview

Lobster provides a typed, JSON-first workflow engine that moves beyond simple text piping to structured data flow. It bridges the gap between "high-level agent reasoning" and "deterministic tool execution."

## Core Capabilities

### 1. Typed Pipelines
- JSON-first data flow between pipeline stages
- Schema validation at each step
- Type-safe intermediate results

### 2. Key Features
- **`approval` gates**: Human-in-the-loop checkpoints for sensitive operations
- **`llm.invoke`**: First-class LLM invocation as a pipeline stage
- **`resume` state**: Long-running workflow persistence and recovery

### 3. Strategic Value
- **Token Reduction**: Eliminates repetitive planning cycles for known tasks
- **Reliability**: Deterministic execution with explicit state management
- **Cost Optimization**: Reduces LLM calls for standardized workflows

## Integration Architecture

### Current State (Agent-Planned)
```
Agent Plans → Execute → Observe → Re-plan → Execute → ...
```

### Evolved State (Lobster-Driven)
```
Agent Invokes Lobster Workflow → Lobster Handles State/Approvals/Retries → Agent Receives Result
```

## Implementation Pattern

### Step 1: Workflow Definition
```typescript
// workflows/lobster-schema.ts
export interface LobsterWorkflow {
  id: string;
  version: string;
  stages: WorkflowStage[];
  approvals?: ApprovalGate[];
}

export interface WorkflowStage {
  type: 'llm.invoke' | 'tool.call' | 'approval' | 'transform';
  input: JsonSchema;
  output: JsonSchema;
  config?: Record<string, unknown>;
}
```

### Step 2: Skill Registration
```typescript
// skills/lobster-bridge/skill.ts
export const lobsterSkill = {
  name: 'lobster-workflow',
  description: 'Execute deterministic Lobster workflows',
  
  async execute(workflowId: string, inputs: unknown) {
    const workflow = await workflowRegistry.get(workflowId);
    const runner = new LobsterRunner(workflow);
    
    return runner.execute(inputs, {
      onApproval: (gate) => this.requestHumanApproval(gate),
      onResume: (state) => this.persistState(state)
    });
  }
};
```

### Step 3: ClawHub Integration
```typescript
// clawhub/lobster-skill-pattern.ts
export function registerLobsterSkill(workflow: LobsterWorkflow): Skill {
  return {
    id: `lobster:${workflow.id}`,
    invoke: async (ctx, inputs) => {
      const runner = ctx.lobster.createRunner(workflow);
      return runner.execute(inputs);
    },
    metadata: {
      requiresApproval: workflow.approvals?.length > 0,
      estimatedLatency: estimateLatency(workflow)
    }
  };
}
```

## Unified Workflow Registry

### Concept
Create a shared registry where Lobster workflows can be published as "Agent Skills," making complex multi-step processes discoverable via ClawHub.

### Registry Structure
```typescript
// registry/unified-workflow-registry.ts
interface WorkflowEntry {
  id: string;
  type: 'lobster' | 'agentic' | 'hybrid';
  definition: LobsterWorkflow | AgentSkill;
  metadata: {
    author: string;
    version: string;
    tags: string[];
    estimatedCost: number;
    averageLatency: number;
  };
  discovery: {
    naturalLanguageQueries: string[];
    keywords: string[];
  };
}
```

## Example Workflows

### Simple Approval Workflow
```yaml
# workflows/expense-approval.yaml
id: expense-approval
version: "1.0.0"
stages:
  - type: transform
    name: extract-expense
    input:
      receipt: image
    output:
      amount: number
      category: string
      
  - type: approval
    name: manager-approval
    condition: amount > 100
    approvers: [manager@company.com]
    
  - type: llm.invoke
    name: generate-summary
    prompt: "Summarize this expense: {{amount}} for {{category}}"
```

### Hybrid Agentic Workflow
```yaml
# workflows/research-pipeline.yaml
id: research-pipeline
version: "1.0.0"
stages:
  - type: llm.invoke
    name: plan-research
    # Agent plans the research approach
    
  - type: tool.call
    name: search-sources
    tool: web-search
    
  - type: llm.invoke
    name: synthesize-findings
    # Agent synthesizes results
    
  - type: approval
    name: publish-check
    condition: confidence < 0.8
```

## Migration Path

### From Agent-Planned to Lobster

1. **Identify Repetitive Patterns**
   - Analyze session logs for common multi-step tasks
   - Extract workflows with >3 similar executions

2. **Formalize as Lobster Workflows**
   - Define inputs/outputs for each stage
   - Identify approval points
   - Map to existing tools/skills

3. **Publish to ClawHub**
   - Register as discoverable skill
   - Add natural language triggers
   - Monitor adoption metrics

## Security Considerations

- **Approval Gates**: Require human confirmation for sensitive operations
- **Scope Isolation**: Workflows run with minimal required permissions
- **Audit Trail**: All workflow executions logged with full state history
- **Sandboxing**: Lobster workflows execute in isolated contexts

## Related Patterns

- `visionclaw_fast_path_pattern.md` - Similar deterministic execution for VisionClaw
- `neural_fitness_weighting.md` - Neural-aware workflow selection
- `circuit_breaker_pattern.md` - Resilience for workflow execution

## Action Items

- [ ] Formalize the "Lobster Skill" pattern for ClawHub
- [ ] Create MCP server for AION astronomical data modality
- [ ] Implement workflow registry with discovery capabilities
- [ ] Add Lobster runner integration to OpenClaw Gateway
