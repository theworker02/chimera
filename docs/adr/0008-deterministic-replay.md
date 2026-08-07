# ADR-0008: Deterministic execution & replay

## Status
Accepted

## Context
Fault isolation and neighbor recovery require bit-stable state transitions across heterogeneous CPUs.

## Decision
- Append-only **BLAKE3-chained TxLog** for task mutations; recovery = verify + replay.
- Immutable sealed memory regions in CNK.
- Consensus math prefers **Q16.16 FixedPoint**; SoftF32 canonicalizes NaN/−0 but is not claimed bit-identical across all FPUs for long chains.
- Degradation policy downsamples / forces fixed-point on low-RAM / no-FPU profiles instead of dropping tasks.

## Consequences
- Host tests prove replay recovery.
- Cross-arch float consensus should use FixedPoint, not host `f32` sin chains.
