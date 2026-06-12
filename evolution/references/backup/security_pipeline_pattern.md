# Security Pipeline Pattern

**Source:** OpenEvolve Night Cycle Report 2026-04-12 04:33 (ClawHub Analysis)  
**Author:** Pattern identified from commits 8fcd53f, ba2c73e, 0708a43  
**Priority:** P1 - High Priority  
**Classification:** Security Pattern / Validation Pattern

---

## Problem Statement

**Scattered Security Validations:** Security checks are distributed across multiple locations in ClawHub skill scanning:
- Resource ID exposure detection
- Template injection detection  
- Soft-delete validation
- CJK input sanitization

**Problems:**
- Duplicated logic across handlers
- Inconsistent validation order
- Hard to maintain and audit
- Easy to miss security checks

---

## Solution: Unified Security Validation Pipeline

### Core Concept

Centralize all security validations into a reusable pipeline:

```
Request → [Resource ID Scan] → [Template Injection Check] → [Soft-Delete Validation] → [CJK Sanitization] → [Proceed]
              ↓                      ↓                         ↓                      ↓
         (fail fast)            (fail fast)              (fail fast)            (fail fast)
```

### Implementation

#### 1. Pipeline Stage Interface

```typescript
// src/security/pipeline/types.ts

export interface SecurityStage {
  name: string;
  priority: number;  // Lower = earlier
  execute: (input: SecurityInput) => Promise<SecurityResult>;
}

export interface SecurityInput {
  // Skill metadata
  skillId?: string;
  skillName?: string;
  sourceCode?: string;
  
  // Request context
  userId?: string;
  action?: string;
  
  // Data to validate
  resourceIds?: string[];
  templateContent?: string;
  softDeleteStatus?: boolean;
  cjkInput?: string;
  
  // Additional context
  metadata?: Record<string, unknown>;
}

export interface SecurityResult {
  pass: boolean;
  stageName: string;
  severity?: 'info' | 'warning' | 'error' | 'critical';
  message?: string;
  details?: Record<string, unknown>;
}

export interface PipelineResult {
  valid: boolean;
  passedStages: string[];
  failedStage?: SecurityResult;
  executionTimeMs: number;
}
```

#### 2. Security Pipeline Implementation

```typescript
// src/security/pipeline/pipeline.ts

export class SecurityPipeline {
  private stages: SecurityStage[] = [];
  private metrics: PipelineMetrics;

  constructor(config: PipelineConfig = {}) {
    this.metrics = new PipelineMetrics();
    
    // Register default stages
    if (config.defaultStages !== false) {
      this.registerDefaultStages();
    }
  }

  /**
   * Register a security validation stage
   */
  registerStage(stage: SecurityStage): void {
    this.stages.push(stage);
    // Sort by priority
    this.stages.sort((a, b) => a.priority - b.priority);
  }

  /**
   * Execute all security stages
   */
  async execute(input: SecurityInput): Promise<PipelineResult> {
    const startTime = Date.now();
    const passedStages: string[] = [];

    for (const stage of this.stages) {
      try {
        const result = await stage.execute(input);

        if (!result.pass) {
          // Log security violation
          this.logViolation(result, input);
          
          return {
            valid: false,
            passedStages,
            failedStage: result,
            executionTimeMs: Date.now() - startTime,
          };
        }

        passedStages.push(stage.name);
        
      } catch (error) {
        // Stage threw unexpected error
        const err = error instanceof Error ? error : new Error(String(error));
        console.error(`Security stage ${stage.name} failed:`, err);
        
        return {
          valid: false,
          passedStages,
          failedStage: {
            pass: false,
            stageName: stage.name,
            severity: 'critical',
            message: `Stage execution error: ${err.message}`,
          },
          executionTimeMs: Date.now() - startTime,
        };
      }
    }

    return {
      valid: true,
      passedStages,
      executionTimeMs: Date.now() - startTime,
    };
  }

  /**
   * Execute stages with specific names only
   */
  async executeStages(
    input: SecurityInput,
    stageNames: string[]
  ): Promise<PipelineResult> {
    const filteredStages = this.stages.filter(s => stageNames.includes(s.name));
    const tempPipeline = new SecurityPipeline({ defaultStages: false });
    
    for (const stage of filteredStages) {
      tempPipeline.registerStage(stage);
    }
    
    return tempPipeline.execute(input);
  }

  private registerDefaultStages(): void {
    this.registerStage(new ResourceIdExposureStage());
    this.registerStage(new TemplateInjectionStage());
    this.registerStage(new SoftDeleteValidationStage());
    this.registerStage(new CjkSanitizationStage());
  }

  private logViolation(result: SecurityResult, input: SecurityInput): void {
    console.warn(`[Security Pipeline] Violation in ${result.stageName}:`, {
      severity: result.severity,
      message: result.message,
      userId: input.userId,
      action: input.action,
      timestamp: new Date().toISOString(),
    });
    
    // Emit metrics
    this.metrics.recordViolation(result.stageName, result.severity);
  }
}
```

#### 3. Individual Security Stages

```typescript
// src/security/pipeline/stages/resource-id-exposure.ts

export class ResourceIdExposureStage implements SecurityStage {
  name = 'resourceIdExposure';
  priority = 10;

  async execute(input: SecurityInput): Promise<SecurityResult> {
    if (!input.resourceIds || input.resourceIds.length === 0) {
      return { pass: true, stageName: this.name };
    }

    const exposedIds: string[] = [];

    for (const resourceId of input.resourceIds) {
      // Check for auto-increment exposure
      if (this.isAutoIncrementExposed(resourceId)) {
        exposedIds.push(resourceId);
      }
      
      // Check for UUID exposure patterns
      if (this.isPredictableUuid(resourceId)) {
        exposedIds.push(resourceId);
      }
    }

    if (exposedIds.length > 0) {
      return {
        pass: false,
        stageName: this.name,
        severity: 'error',
        message: `Resource ID exposure detected: ${exposedIds.join(', ')}`,
        details: { exposedIds },
      };
    }

    return { pass: true, stageName: this.name };
  }

  private isAutoIncrementExposed(id: string): boolean {
    // Check if ID is simple sequential number
    return /^\d{1,10}$/.test(id) && parseInt(id, 10) < 1000000;
  }

  private isPredictableUuid(id: string): boolean {
    // Check for UUID v1 (time-based, predictable)
    return /^[0-9a-f]{8}-[0-9a-f]{4}-1[0-9a-f]{3}/.test(id);
  }
}

// src/security/pipeline/stages/template-injection.ts

export class TemplateInjectionStage implements SecurityStage {
  name = 'templateInjection';
  priority = 20;

  private readonly dangerousPatterns = [
    /\{\{\s*.*\|\s*safe\s*\}\}/g,  // Jinja safe filter
    /<\%[\s\S]*?%>/g,                 // ERB tags
    /\$\{[\s\S]*?\}/g,                // Template literals
    /<%=?[\s\S]*?%>/g,               // Underscore templates
  ];

  async execute(input: SecurityInput): Promise<SecurityResult> {
    if (!input.templateContent) {
      return { pass: true, stageName: this.name };
    }

    const matches: string[] = [];

    for (const pattern of this.dangerousPatterns) {
      const found = input.templateContent.match(pattern);
      if (found) {
        matches.push(...found);
      }
    }

    if (matches.length > 0) {
      return {
        pass: false,
        stageName: this.name,
        severity: 'critical',
        message: 'Potential template injection detected',
        details: { 
          patterns: matches.slice(0, 5),  // Limit details
          count: matches.length,
        },
      };
    }

    return { pass: true, stageName: this.name };
  }
}

// src/security/pipeline/stages/soft-delete-validation.ts

export class SoftDeleteValidationStage implements SecurityStage {
  name = 'softDeleteValidation';
  priority = 30;

  async execute(input: SecurityInput): Promise<SecurityResult> {
    // Check if action involves a soft-deleted resource
    if (input.softDeleteStatus === undefined) {
      return { pass: true, stageName: this.name };
    }

    // Prevent starring, editing, or other actions on soft-deleted items
    const restrictedActions = ['star', 'edit', 'delete', 'publish'];
    
    if (input.softDeleteStatus && input.action && restrictedActions.includes(input.action)) {
      return {
        pass: false,
        stageName: this.name,
        severity: 'error',
        message: `Cannot perform '${input.action}' on soft-deleted resource`,
        details: { 
          action: input.action,
          resourceStatus: 'soft-deleted',
        },
      };
    }

    return { pass: true, stageName: this.name };
  }
}

// src/security/pipeline/stages/cjk-sanitization.ts

export class CjkSanitizationStage implements SecurityStage {
  name = 'cjkSanitization';
  priority = 40;

  // CJK Unicode ranges
  private readonly cjkRanges = [
    { start: 0x4E00, end: 0x9FFF },    // CJK Unified Ideographs
    { start: 0x3400, end: 0x4DBF },    // CJK Extension A
    { start: 0x3040, end: 0x309F },    // Hiragana
    { start: 0x30A0, end: 0x30FF },    // Katakana
    { start: 0xAC00, end: 0xD7AF },    // Hangul Syllables
  ];

  async execute(input: SecurityInput): Promise<SecurityResult> {
    if (!input.cjkInput) {
      return { pass: true, stageName: this.name };
    }

    // Check for homoglyph attacks
    if (this.containsHomoglyphAttack(input.cjkInput)) {
      return {
        pass: false,
        stageName: this.name,
        severity: 'warning',
        message: 'Potential homoglyph attack detected in CJK input',
        details: { input: input.cjkInput.slice(0, 50) },
      };
    }

    // Normalize CJK characters
    const normalized = this.normalizeCjk(input.cjkInput);
    
    // Check length after normalization
    if (normalized.length > 10000) {
      return {
        pass: false,
        stageName: this.name,
        severity: 'error',
        message: 'CJK input exceeds maximum length after normalization',
        details: { length: normalized.length },
      };
    }

    return { pass: true, stageName: this.name };
  }

  private containsHomoglyphAttack(input: string): boolean {
    // Check for mixed script usage that could be homoglyph attacks
    const hasCjk = this.containsCjk(input);
    const hasAscii = /[a-zA-Z]/.test(input);
    const hasSuspiciousConfusables = /[ΑΒΕΖΗΙΚΜΝΟΡΤΧ]/.test(input); // Greek lookalikes
    
    return hasCjk && hasAscii && hasSuspiciousConfusables;
  }

  private containsCjk(input: string): boolean {
    for (const char of input) {
      const code = char.charCodeAt(0);
      for (const range of this.cjkRanges) {
        if (code >= range.start && code <= range.end) {
          return true;
        }
      }
    }
    return false;
  }

  private normalizeCjk(input: string): string {
    // NFKC normalization for compatibility equivalence
    return input.normalize('NFKC');
  }
}
```

---

## Usage Examples

### Skill Validation

```typescript
// routes/skills/create.ts

import { SecurityPipeline } from '../../security/pipeline';

const securityPipeline = new SecurityPipeline();

export async function createSkill(req: Request, res: Response) {
  const result = await securityPipeline.execute({
    skillName: req.body.name,
    sourceCode: req.body.code,
    templateContent: req.body.template,
    userId: req.user.id,
    action: 'create',
  });

  if (!result.valid) {
    return res.status(400).json({
      error: 'Security validation failed',
      stage: result.failedStage?.stageName,
      message: result.failedStage?.message,
    });
  }

  // Proceed with skill creation
  const skill = await Skill.create(req.body);
  res.json(skill);
}
```

### Selective Stage Execution

```typescript
// Validate only specific checks
const result = await securityPipeline.executeStages(
  { cjkInput: searchQuery },
  ['cjkSanitization']
);
```

---

## Configuration

```yaml
# config/security.yaml
security:
  pipeline:
    enabled: true
    
    stages:
      resourceIdExposure:
        enabled: true
        priority: 10
        
      templateInjection:
        enabled: true
        priority: 20
        block-patterns:
          - '{{ .* | safe }}'
          - '<%.*%>'
          
      softDeleteValidation:
        enabled: true
        priority: 30
        restricted-actions:
          - star
          - edit
          - delete
          - publish
          
      cjkSanitization:
        enabled: true
        priority: 40
        max-length: 10000
        detect-homoglyphs: true
```

---

## Testing

```typescript
// test/security-pipeline.test.ts

describe('SecurityPipeline', () => {
  let pipeline: SecurityPipeline;

  beforeEach(() => {
    pipeline = new SecurityPipeline({ defaultStages: false });
  });

  it('should pass all stages for valid input', async () => {
    pipeline.registerStage({
      name: 'testPass',
      priority: 1,
      execute: async () => ({ pass: true, stageName: 'testPass' }),
    });

    const result = await pipeline.execute({});
    
    expect(result.valid).toBe(true);
    expect(result.passedStages).toContain('testPass');
  });

  it('should fail fast on first failure', async () => {
    pipeline.registerStage({
      name: 'testFail',
      priority: 1,
      execute: async () => ({
        pass: false,
        stageName: 'testFail',
        severity: 'error',
        message: 'Test failure',
      }),
    });

    pipeline.registerStage({
      name: 'neverReached',
      priority: 2,
      execute: async () => ({ pass: true, stageName: 'neverReached' }),
    });

    const result = await pipeline.execute({});
    
    expect(result.valid).toBe(false);
    expect(result.failedStage?.stageName).toBe('testFail');
    expect(result.passedStages).not.toContain('neverReached');
  });

  it('should execute stages in priority order', async () => {
    const order: string[] = [];

    pipeline.registerStage({
      name: 'second',
      priority: 2,
      execute: async () => {
        order.push('second');
        return { pass: true, stageName: 'second' };
      },
    });

    pipeline.registerStage({
      name: 'first',
      priority: 1,
      execute: async () => {
        order.push('first');
        return { pass: true, stageName: 'first' };
      },
    });

    await pipeline.execute({});
    
    expect(order).toEqual(['first', 'second']);
  });

  describe('ResourceIdExposureStage', () => {
    const stage = new ResourceIdExposureStage();

    it('should detect auto-increment ID exposure', async () => {
      const result = await stage.execute({
        resourceIds: ['1', '2', '999999'],
      });

      expect(result.pass).toBe(false);
      expect(result.message).toContain('exposure detected');
    });

    it('should pass for UUID v4', async () => {
      const result = await stage.execute({
        resourceIds: ['550e8400-e29b-41d4-a716-446655440000'],
      });

      expect(result.pass).toBe(true);
    });
  });
});
```

---

## Related Patterns

- **Barrel Bypassing Guide**: `barrel_bypassing_guide.md`
- **Narrow Surface Pattern**: `narrow_surface_pattern.md`
- **Security Audit Patterns**: `security_audit_patterns.md`

---

## References

- Night Cycle Report: `night_cycle_20260412_0433.md`
- Commits: `8fcd53f`, `ba2c73e`, `0708a43`
- Pattern: Defensive-First Validation

---

*Generated by OpenEvolve Auto-Apply*  
*Classification: P1 High Priority Security Pattern*  
*Credit: ClawHub security hardening analysis*
