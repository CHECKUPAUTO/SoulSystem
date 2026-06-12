# ESLint Rule: No Plugin Index Imports

**Source:** OpenEvolve Night Cycle Report 2026-04-11 (2115)  
**Purpose:** Automated enforcement of plugin barrel avoidance pattern

## Rule Purpose

Prevent performance regressions by detecting imports from plugin index/barrel files in hot paths. These imports trigger O(n) barrel traversal instead of O(1) direct registry lookups.

## Rule Configuration

```javascript
// eslint-plugin-openclaw/rules/no-plugin-index-imports.js
module.exports = {
  meta: {
    type: 'suggestion',
    docs: {
      description: 'Disallow imports from plugin index files in performance-critical paths',
      category: 'Performance',
      recommended: 'error'
    },
    schema: [
      {
        type: 'object',
        properties: {
          restrictedPaths: {
            type: 'array',
            items: { type: 'string' },
            description: 'Glob patterns for restricted import sources'
          },
          allowedPaths: {
            type: 'array',
            items: { type: 'string' },
            description: 'Glob patterns for files exempt from this rule'
          }
        },
        additionalProperties: false
      }
    ],
    messages: {
      noPluginIndex: 
        "Import from '{{path}}' detected. " +
        "Use direct registry import instead for O(1) lookup. " +
        "See: https://github.com/openclaw/openclaw/blob/main/docs/performance/barrel-avoidance.md"
    }
  },
  
  create(context) {
    const options = context.options[0] || {};
    const restrictedPaths = options.restrictedPaths || [
      '**/channels/plugins/index.js',
      '**/channels/plugins/index.ts',
      '**/channels/plugins/bundled.js',
      '**/plugin-sdk/*/index.js',
    ];
    const allowedPaths = options.allowedPaths || [
      '**/*.test.ts',
      '**/test/**',
    ];
    
    const filename = context.getFilename();
    
    // Skip allowed paths
    if (allowedPaths.some(pattern => 
      require('minimatch')(filename, pattern))) {
      return {};
    }
    
    function checkImport(node, importPath) {
      if (restrictedPaths.some(pattern => 
        require('minimatch')(importPath, pattern))) {
        context.report({
          node,
          messageId: 'noPluginIndex',
          data: { path: importPath }
        });
      }
    }
    
    return {
      ImportDeclaration(node) {
        checkImport(node, node.source.value);
      },
      CallExpression(node) {
        // Check require() calls
        if (node.callee.name === 'require' && 
            node.arguments.length > 0 &&
            node.arguments[0].type === 'Literal') {
          checkImport(node, node.arguments[0].value);
        }
      }
    };
  }
};
```

## .eslintrc Configuration

```javascript
// .eslintrc.js
module.exports = {
  plugins: ['openclaw'],
  rules: {
    'openclaw/no-plugin-index-imports': ['error', {
      restrictedPaths: [
        // Plugin barrel files
        '**/channels/plugins/index.js',
        '**/channels/plugins/index.ts',
        '**/channels/plugins/bundled.js',
        '**/channels/plugins/bundled.ts',
        
        // SDK barrel files  
        '@openclaw/plugin-sdk',
        '@openclaw/plugin-sdk/reply-payload',
        
        // Runtime barrel files
        '**/providers/*/index.js',
        '**/extensions/*/index.js',
      ],
      allowedPaths: [
        // Test files can use barrel imports
        '**/*.test.ts',
        '**/*.test.js',
        '**/test/**',
        '**/__tests__/**',
        
        // Plugin registration files
        '**/channels/plugins/register.ts',
        
        // Scripts and build tools
        'scripts/**',
      ]
    }],
    
    // Complementary rule for direct registry imports
    'openclaw/prefer-registry-import': ['warn', {
      registryPaths: [
        { from: '**/channels/plugins/index.js', to: '**/channels/registry.js' },
        { from: '**/plugin-sdk', to: '**/channels/registry.js' },
      ]
    }]
  }
};
```

## Auto-Fix Implementation

```javascript
// Auto-fix for known mappings
function getRegistryAlternative(importPath) {
  const mappings = {
    '../../channels/plugins/index.js': '../../channels/registry.js',
    '@openclaw/plugin-sdk': '../../channels/registry.js',
    // Add more as needed
  };
  
  return mappings[importPath] || null;
}

// In the rule's create() function, add fixer:
fix(fixer) {
  const alternative = getRegistryAlternative(importPath);
  if (alternative) {
    return fixer.replaceText(
      node.source || node.arguments[0],
      `'${alternative}'`
    );
  }
  return null;
}
```

## CI Integration

```yaml
# .github/workflows/lint.yml
name: Lint

on: [push, pull_request]

jobs:
  eslint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
      - run: npm ci
      - run: npx eslint --rule 'openclaw/no-plugin-index-imports: error' src/
```

## Migration Script

```bash
#!/bin/bash
# scripts/migrate-barrel-imports.sh

# Find all plugin index imports
echo "Finding plugin index imports..."
grep -r "from.*channels/plugins/index" src/ --include="*.ts" --include="*.js" | grep -v test | grep -v node_modules

echo ""
echo "Suggested replacements:"
echo "  FROM: import { normalizeChannelId } from '../../channels/plugins/index.js'"
echo "  TO:   import { normalizeAnyChannelId } from '../../channels/registry.js'"
```

## Benefits

1. **Prevents Regressions:** Automated detection of performance anti-patterns
2. **Clear Errors:** Descriptive messages with links to documentation
3. **Auto-Fixable:** Known mappings can be auto-corrected
4. **Configurable:** Project-specific allow/restrict lists

## Testing the Rule

```javascript
// tests/rules/no-plugin-index-imports.test.js
const { RuleTester } = require('eslint');
const rule = require('../../rules/no-plugin-index-imports');

const ruleTester = new RuleTester({
  parserOptions: { ecmaVersion: 2020, sourceType: 'module' }
});

ruleTester.run('no-plugin-index-imports', rule, {
  valid: [
    // Direct registry import
    "import { normalizeAnyChannelId } from '../../channels/registry.js';",
    // Test files allowed
    { code: "import { x } from '../../channels/plugins/index.js';", 
      filename: 'src/test/utils.ts' }
  ],
  invalid: [
    {
      code: "import { normalizeChannelId } from '../../channels/plugins/index.js';",
      errors: [{ messageId: 'noPluginIndex' }]
    },
    {
      code: "const x = require('../../channels/plugins/index.js');",
      errors: [{ messageId: 'noPluginIndex' }]
    }
  ]
});
```

## References

- Night Cycle Report: night_cycle_20260411_2115.md
- Pattern: plugin_avoidance_pattern_2026-04-11.md
