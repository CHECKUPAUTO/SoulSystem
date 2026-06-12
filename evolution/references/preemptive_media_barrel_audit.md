# Preemptive Barrel Audit for Media Modules

**Source:** Night Cycle 2026-04-13 03:30
**Priority:** P1
**Status:** Proposed

## Context
The video-generation module grew by +617 lines this cycle (providerOptions, inputAudios, imageRoles). Music-generation is also expanding. Core modules required a 24-commit barrel bypass campaign after hitting complexity thresholds.

## Recommendation
Before media modules hit the same threshold:
1. Map all cross-module imports in `src/video-generation/` and `src/music-generation/`
2. Identify barrel imports that can be replaced with direct paths
3. Apply the established pattern: `perf: import X directly`

## Pattern Reference
From barrel_bypassing_guide.md:
```
fix(cycles): bypass context engine and config barrels
fix(cycles): bypass store and channel barrels
fix(cycles): narrow channel registry imports
```

## Estimated Effort
1-2 focused sessions per module.