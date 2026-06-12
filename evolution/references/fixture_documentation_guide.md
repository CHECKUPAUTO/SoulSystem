# Fixture Documentation Generator Pattern

## Overview
Automated documentation generation for test fixtures improves discoverability and onboarding.

## Pattern

```typescript
// src/test/fixtures/doc-generator.ts
export interface FixtureDoc {
  name: string;
  purpose: string;
  usage: string[];
}

export function generateFixtureDocs(): FixtureDoc[] {
  const fixtures = import.meta.glob('../**/fixtures/*.ts');
  return fixtures.map(f => ({
    name: extractName(f),
    purpose: inferPurpose(f),
    usage: generateExamples(f)
  }));
}
```

## Usage
- Generate docs on CI pipeline
- Auto-update README from fixture metadata
- Create interactive fixture explorer

## Benefits
- Automatic test documentation
- Onboarding improvements
- Better code searchability
