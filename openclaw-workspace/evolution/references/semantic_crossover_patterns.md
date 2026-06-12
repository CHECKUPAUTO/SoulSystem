# Semantic Crossover Patterns

**Source:** OpenEvolve Night Cycle Report 2026-04-12 03:01  
**Priority:** P2  
**Used In:** IronReview T430 Algorithm

---

## Overview

Semantic crossover in evolutionary code optimization performs line-based crossover while respecting semantic boundaries. This ensures code remains syntactically valid and semantically meaningful after recombination.

---

## Core Pattern

### Semantic Boundary Detection

```typescript
// Semantic boundaries - safe crossover points
const SEMANTIC_BOUNDARIES = [
  // Function boundaries
  /^function\s+\w+\s*\(/,
  /^async\s+function\s+\w+\s*\(/,
  /^const\s+\w+\s*=\s*(async\s*)?\(/,

  // Class boundaries
  /^class\s+\w+/,
  /^constructor\s*\(/,
  /^\s*(private|public|protected|static)?\s*\w+\s*\([^)]*\)\s*{/,

  // Block boundaries
  /^\s*if\s*\(/,
  /^\s*for\s*\(/,
  /^\s*while\s*\(/,
  /^\s*switch\s*\(/,
  /^\s*try\s*{/,
  /^\s*catch\s*\(/,
  /^\s*finally\s*{/,

  // Import/export boundaries
  /^import\s+/,
  /^export\s+/,

  // Interface/type boundaries
  /^interface\s+\w+/,
  /^type\s+\w+\s*=/,
];

function findSemanticBoundaries(code: string): number[] {
  const lines = code.split('\n');
  const boundaries: number[] = [0]; // Start of file

  for (let i = 0; i < lines.length; i++) {
    for (const pattern of SEMANTIC_BOUNDARIES) {
      if (pattern.test(lines[i])) {
        boundaries.push(i);
        break;
      }
    }
  }

  boundaries.push(lines.length); // End of file
  return boundaries.sort((a, b) => a - b);
}
```

### Semantic Crossover Operation

```typescript
interface CrossoverPoint {
  line: number;
  confidence: number; // 0-1, higher = safer boundary
}

function semanticCrossover(
  parentA: string,
  parentB: string
): [string, string] {
  const boundariesA = findSemanticBoundaries(parentA);
  const boundariesB = findSemanticBoundaries(parentB);

  // Select crossover point (must exist in both)
  const validPoints = boundariesA.filter(b =>
    boundariesB.includes(b)
  );

  if (validPoints.length < 2) {
    // Not enough valid boundaries, return clones
    return [parentA, parentB];
  }

  const crossoverLine = validPoints[
    Math.floor(Math.random() * (validPoints.length - 1)) + 1
  ];

  // Split at semantic boundary
  const linesA = parentA.split('\n');
  const linesB = parentB.split('\n');

  const childA = [
    ...linesA.slice(0, crossoverLine),
    ...linesB.slice(crossoverLine),
  ].join('\n');

  const childB = [
    ...linesB.slice(0, crossoverLine),
    ...linesA.slice(crossoverLine),
  ].join('\n');

  return [childA, childB];
}
```

---

## Advanced Patterns

### Weighted Boundary Selection

```typescript
function calculateBoundaryConfidence(
  code: string,
  line: number
): number {
  const lines = code.split('\n');
  const lineContent = lines[line];
  let confidence = 0.5;

  // Higher confidence for function boundaries
  if (/^function|^async function/.test(lineContent)) {
    confidence += 0.3;
  }

  // Higher confidence for complete blocks
  if (lineContent.includes('{') && !lineContent.includes('}')) {
    confidence += 0.1;
  }

  // Lower confidence for mid-block lines
  const indentation = lineContent.match(/^(\s*)/)?.[1].length || 0;
  if (indentation > 0 && !SEMANTIC_BOUNDARIES.some(p => p.test(lineContent))) {
    confidence -= 0.2;
  }

  return Math.max(0, Math.min(1, confidence));
}

function selectWeightedCrossoverPoint(
  boundaries: number[],
  code: string
): number {
  const weights = boundaries.map(b =>
    calculateBoundaryConfidence(code, b)
  );

  const totalWeight = weights.reduce((a, b) => a + b, 0);
  let random = Math.random() * totalWeight;

  for (let i = 0; i < boundaries.length; i++) {
    random -= weights[i];
    if (random <= 0) {
      return boundaries[i];
    }
  }

  return boundaries[boundaries.length - 1];
}
```

### Multi-Point Semantic Crossover

```typescript
function multiPointSemanticCrossover(
  parentA: string,
  parentB: string,
  numPoints: number = 2
): [string, string] {
  const boundariesA = findSemanticBoundaries(parentA);
  const commonBoundaries = boundariesA.filter(b =>
    findSemanticBoundaries(parentB).includes(b)
  );

  if (commonBoundaries.length < numPoints + 1) {
    return semanticCrossover(parentA, parentB);
  }

  // Select multiple points
  const points = commonBoundaries
    .sort(() => Math.random() - 0.5)
    .slice(0, numPoints)
    .sort((a, b) => a - b);

  // Perform alternating crossover
  const linesA = parentA.split('\n');
  const linesB = parentB.split('\n');

  const segmentsA: string[][] = [];
  const segmentsB: string[][] = [];

  let lastPoint = 0;
  for (const point of [...points, linesA.length]) {
    segmentsA.push(linesA.slice(lastPoint, point));
    segmentsB.push(linesB.slice(lastPoint, point));
    lastPoint = point;
  }

  // Alternate segments
  const childA: string[] = [];
  const childB: string[] = [];

  for (let i = 0; i < segmentsA.length; i++) {
    if (i % 2 === 0) {
      childA.push(...segmentsA[i]);
      childB.push(...segmentsB[i]);
    } else {
      childA.push(...segmentsB[i]);
      childB.push(...segmentsA[i]);
    }
  }

  return [childA.join('\n'), childB.join('\n')];
}
```

---

## Language-Specific Adaptations

### TypeScript/Type-Aware Crossover

```typescript
const TS_SPECIFIC_BOUNDARIES = [
  // Type definitions
  /^type\s+\w+</,  // Generic types
  /^interface\s+\w+</,
  /^enum\s+\w+/,

  // Decorators
  /^\s*@\w+/,

  // Generic function boundaries
  /^function\s+\w+\s*</,
  /^const\s+\w+\s*=\s*<\s*\w+\s*>/,

  // Namespace/module
  /^namespace\s+\w+/,
  /^module\s+\w+/,
];

function findTypeScriptBoundaries(code: string): number[] {
  const baseBoundaries = findSemanticBoundaries(code);
  const tsBoundaries = findBoundariesByPatterns(code, TS_SPECIFIC_BOUNDARIES);

  return [...new Set([...baseBoundaries, ...tsBoundaries])].sort((a, b) => a - b);
}
```

### Rust Crossover (for IronReview)

```rust
const RUST_BOUNDARIES: &[Regex] = &[
    // Function signatures
    regex!(r"^\s*(pub\s+)?(async\s+)?(unsafe\s+)?fn\s+\w+"),
    // Impl blocks
    regex!(r"^\s*(pub\s+)?impl\s+"),
    // Trait definitions
    regex!(r"^\s*(pub\s+)?trait\s+\w+"),
    // Struct/enum definitions
    regex!(r"^\s*(pub\s+)?(struct|enum)\s+\w+"),
    // Match arms
    regex!(r"^\s*[\w|_]+\s*=>"),
    // Macro definitions
    regex!(r"^\s*macro_rules!\s+\w+"),
];

pub fn find_rust_boundaries(code: &str) -> Vec<usize> {
    // Implementation similar to TypeScript version
}
```

---

## Validation

### Post-Crossover Validation

```typescript
interface ValidationResult {
  valid: boolean;
  errors: string[];
}

function validateOffspring(code: string): ValidationResult {
  const errors: string[] = [];

  // Check for unmatched braces
  const openBraces = (code.match(/\{/g) || []).length;
  const closeBraces = (code.match(/\}/g) || []).length;
  if (openBraces !== closeBraces) {
    errors.push(`Unmatched braces: ${openBraces} open, ${closeBraces} close`);
  }

  // Check for unmatched parentheses
  const openParens = (code.match(/\(/g) || []).length;
  const closeParens = (code.match(/\)/g) || []).length;
  if (openParens !== closeParens) {
    errors.push(`Unmatched parens: ${openParens} open, ${closeParens} close`);
  }

  // Check for basic TypeScript syntax errors
  try {
    // Use TypeScript compiler API for validation
    const sourceFile = ts.createSourceFile(
      'temp.ts',
      code,
      ts.ScriptTarget.Latest
    );
    
    const diagnostics = ts.getPreEmitDiagnostics(
      ts.createProgram(['temp.ts'], {}, undefined, undefined, sourceFile)
    );

    for (const diagnostic of diagnostics) {
      errors.push(ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n'));
    }
  } catch {
    // If TS parsing fails, it's definitely invalid
    errors.push('TypeScript parsing failed');
  }

  return {
    valid: errors.length === 0,
    errors,
  };
}
```

---

## Integration with IronReview

```typescript
// IronReview T430 uses semantic crossover
class T430Engine {
  crossover(parentA: CodeIndividual, parentB: CodeIndividual): [CodeIndividual, CodeIndividual] {
    // Use semantic crossover
    const [childACode, childBCode] = multiPointSemanticCrossover(
      parentA.source,
      parentB.source,
      this.config.crossoverPoints
    );

    // Validate offspring
    const validationA = validateOffspring(childACode);
    const validationB = validateOffspring(childBCode);

    // Return valid offspring, or parents if invalid
    return [
      validationA.valid ? new CodeIndividual(childACode) : parentA,
      validationB.valid ? new CodeIndividual(childBCode) : parentB,
    ];
  }
}
```

---

## References

- Source Report: `night_cycle_20260412_0301.md`
- IronReview T430: `ironreview_t430_integration.md`
- Related Pattern: `semantic_crossover_patterns.md`
