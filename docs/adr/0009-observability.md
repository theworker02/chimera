# ADR-0009: OpenTelemetry & Prometheus observability

## Status
Accepted

## Context
Enterprise operators need mesh health, task throughput, and latency visibility without drowning nodes in telemetry.

## Decision
- Prometheus text exposition at `/metrics` (always with `mgmt` feature).
- OpenTelemetry OTLP export behind `--features otel` + `--otlp-endpoint`.
- Default trace sample ratio **5%** (`ParentBased` + `TraceIdRatioBased`) targeting **&lt;2% CPU** overhead at typical load.
- Spans on critical paths: `task.execute`, HTTP management, transport classes.

## Consequences
Default builds stay lean. Full OTEL stack is opt-in.
