# ADR-0021: Continuity & hot failover

## Status
Accepted (Phase 11)

## Decision
- `ContinuityPlane` replicates Wasm frames + memory segments to N in-process peers.
- Partition tests recover latest replica after killing a majority of holders.
- Raft KV replication complements frame continuity for shared state.

## Honesty — “zero packet loss”
We demonstrate **zero data loss** via replicated logs + deterministic replay equality checks.
We do **not** claim lossless UDP/QUIC delivery on the wire.
