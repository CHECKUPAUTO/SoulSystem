# Cron: nightly-insights Error Tracking

> Created from night_cycle_20260414_1402.md (Cycle 131)

## Issue

The `nightly-insights` cron job is failing. Likely cause: the `kimi-k2.5` model is unavailable or misconfigured.

**Status**: ❌ Error  
**Schedule**: 08:00 daily  
**Suspected cause**: Model `kimi-k2.5` unavailable for Ollama

## Recommended Actions

1. Check the cron configuration for nightly-insights
2. Verify model availability: `ollama list | grep kimi`
3. Either:
   - Pull the model: `ollama pull kimi-k2.5`
   - Or change the cron to use an available model (e.g., `glm-5.1:cloud`)

## Source Reports

- `night_cycle_20260414_1402.md` — Cycle 131: first identification of nightly-insights cron failure

## Last Updated

2026-04-14T15:46:00+02:00 — Auto-apply cycle (new tracking doc for nightly-insights cron error)