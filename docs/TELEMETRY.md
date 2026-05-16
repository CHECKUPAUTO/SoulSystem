# Telemetry (OpenTelemetry)

## Configuration

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
```

## Visualisation

1. Lancer Jaeger : `docker run -p 16686:16686 -p 4317:4317 jaegertracing/all-in-one`
2. Ouvrir http://localhost:16686

## Spans instrumentées

- `Clawd::process_message`
- `SoulLink::tick`
- `AVID::search`
- `SciRust::compute`
