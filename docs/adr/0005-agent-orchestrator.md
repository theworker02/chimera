# ADR-0005: Intent-driven agent orchestrator

## Status
Accepted

## Context
Operators want declarative jobs (“latency&lt;200ms privacy=local render=hd”) and self-healing under thermal/congestion pressure.

## Decision
Each node runs a **rule-based scoring agent** on a telemetry ring buffer (&lt;1ms decisions). Intents compile into Wasm task plans + ChimeraMEM page budgets. Economy layer issues **ed25519 + BLAKE3 compute receipts**; optional `--features zk-receipts` stubs ZK proofs without default heavy deps.

## Consequences
- No mandatory ML/ZK build cost on Windows.
- Pre-emptive migration when healing pressure rises.
- Receipt verification gates acceptance of completed slices.
