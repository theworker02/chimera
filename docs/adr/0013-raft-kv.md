# ADR-0013: Raft-replicated KV storage

## Status
Accepted (Phase 9)

## Context
Functions and the control plane need strongly consistent shared state. openraft was evaluated; a compact in-tree Raft core was preferred for Windows CI simplicity and zero extra native deps.

## Decision
- Ship a compact Raft implementation (`src/raft_kv.rs`): leader election, log replication, commit/apply, single- and multi-node tests.
- `KvStore` wraps a shared `RaftNode`; single-node lab mode commits immediately; multi-node uses `replicate_to`.
- Expose KV via REST (`/v1/kv`) and optional Wasm host imports (`chimera.kv_get_i32` / `chimera.kv_set_i32`).
- **SQL / relational layer:** not shipped. Prefer correct Raft KV + secondary indexes later. SQL is roadmap.

## Honesty
Network transport over QUIC for Raft RPCs is pluggable/hooks-ready; unit tests use in-process replication. Production mesh wiring of Raft over QUIC remains incremental.

## Consequences
Correctness tests gate merges. Do not advertise SQL until a feature-gated engine exists.
