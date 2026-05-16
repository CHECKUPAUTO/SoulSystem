# JIT Compilation HNN (Cranelift)

## Principe

Les boucles critiques du HNN Mesh sont compilées en code natif
via Cranelift pour accélérer les ticks/seconde.

## Activation

```bash
cargo build --features jit
```

## Cache

Le code JIT est mis en cache dans `/tmp/soulsystem/jit_cache`.
