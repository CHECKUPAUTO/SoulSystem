# Provider Schema Consistency CI Check

**Date:** 2026-04-13  
**Priority:** P2  
**Origin:** night_cycle_20260413_0019

## Pattern

The video generation module uses `AssertAssignable<SchemaA, SchemaB>` for compile-time SDK type compatibility verification. This pattern should be extended across all provider-SDK boundaries.

## Proposal

Create a CI workflow that verifies provider option schemas match SDK types across all providers:

```yaml
# .github/workflows/provider-schema-check.yml
name: Provider Schema Consistency
on: [pull_request]
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Verify provider schemas match SDK types
        run: npx tsc --noEmit --strict
        # AssertAssignable patterns will fail compile if drift occurs
```

## Rationale

- Google Veo already had a bug where `numberOfVideos` was sent but unsupported (#64723)
- `AssertAssignable` catches type drift at compile time
- Currently only applied to video generation — should be universal
- Prevents silent API contract violations between OpenClaw and provider SDKs

## Providers to Cover

- OpenAI (Codex, GPT models)
- Google (Gemini, Veo, Imagen)
- Anthropic (Claude)
- fal (Seedance, video models)
- HeyGen (video agents)

## Related

- `provider_options_validation_pattern.md` — runtime validation pattern
- `video_generation_role_pattern.md` — role-based asset pipeline