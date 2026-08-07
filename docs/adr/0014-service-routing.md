# ADR-0014: Latency-aware service routing

## Status
Accepted (Phase 9)

## Context
Function invocations must find a healthy peer hosting the named service without central load balancers.

## Decision
- Maintain a userspace service registry (`src/registry.rs`): `tenant/function` → peer instances with latency, headroom, and heartbeat TTL.
- Route by score `latency_ms * (1.1 - headroom)`; failover via `route_failover` skipping failed peers.
- Heartbeats refresh entries; expired instances drop (self-healing registry).
- Integrate Phase 4 telemetry into autoscaler / traffic shedder for scale and admit decisions.

## Honesty — “anycast”
This is **userspace peer selection**, not IP anycast or BGP. Clients (gateway) pick the best peer; there is no kernel/network anycast address.

## Consequences
Docs and CLI must say “lowest-latency peer routing”, not “anycast IP”.
