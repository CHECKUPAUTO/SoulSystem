# TypeScript Project References Guide

**Source:** OpenEvolve Night Cycle Report 2026-04-12  
**Purpose:** Formalize plugin boundary enforcement at compile-time instead of script-based checks

## Overview

The current OpenClaw codebase uses script-based boundary checks (`check-extension-import-boundary.mjs`) to enforce plugin SDK boundaries. This guide documents how to migrate to TypeScript Project References for compile-time enforcement.

## Current State

- Manual boundary checks in CI/scripts
- Runtime errors for boundary violations
- Delayed feedback (CI failure vs IDE error)

## Target State

- Compile-time boundary enforcement
- IDE immediate feedback
- TypeScript compiler handles restrictions
- Better separation of concerns

## Implementation Steps

### 1. Create Base tsconfig.json

```json
{
  "compilerOptions": {
    "composite": true,
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true,
    "strict": true
  },
  "references": [
    { "path": "./src/core" },
    { "path": "./src/plugin-sdk" },
    { "path": "./src/extensions" }
  ]
}
```

### 2. Plugin SDK Configuration

```json
// src/plugin-sdk/tsconfig.json
{
  "extends": "../../tsconfig.base.json",
  "compilerOptions": {
    "outDir": "./dist",
    "rootDir": "./src",
    "tsBuildInfoFile": "./dist/.tsbuildinfo"
  },
  "include": ["src/**/*"],
  "exclude": ["**/*.test.ts", "**/*.spec.ts"]
}
```

### 3. Extension Configuration

```json
// src/extensions/tsconfig.json
{
  "extends": "../../tsconfig.base.json",
  "compilerOptions": {
    "outDir": "./dist",
    "rootDir": "./src"
  },
  "references": [
    { "path": "../plugin-sdk" }
  ],
  "include": ["src/**/*"]
}
```

### 4. ESLint Plugin Import Rules

```javascript
// .eslintrc.js
module.exports = {
  rules: {
    'import/no-restricted-paths': ['error', {
      zones: [
        {
          target: './src/extensions/**',
          from: './src/core/**',
          message: 'Extensions cannot import from core directly. Use @openclaw/plugin-sdk.'
        },
        {
          target: './src/plugin-sdk/**',
          from: './src/extensions/**',
          message: 'Plugin SDK cannot depend on extensions.'
        }
      ]
    }],
    'import/no-internal-modules': ['error', {
      allow: ['@openclaw/plugin-sdk/**']
    }]
  }
};
```

## Migration Checklist

- [ ] Enable `composite: true` in base tsconfig
- [ ] Create separate tsconfig for each boundary
- [ ] Add project references
- [ ] Configure ESLint import restrictions
- [ ] Update CI to use `tsc --build`
- [ ] Remove `check-extension-import-boundary.mjs`
- [ ] Update documentation

## Benefits

1. **Immediate Feedback:** IDE shows errors before commit
2. **Faster CI:** TypeScript handles enforcement, no custom scripts
3. **Better Caching:** Incremental builds with `--build`
4. **Clear Dependencies:** Explicit graph in tsconfig files

## Risks

- Initial setup complexity
- IDE configuration required
- Build script updates needed

## References

- [TypeScript Project References](https://www.typescriptlang.org/docs/handbook/project-references.html)
- [ESLint Plugin Import](https://github.com/import-js/eslint-plugin-import)
