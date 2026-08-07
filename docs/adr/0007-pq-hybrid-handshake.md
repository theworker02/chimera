# ADR-0007: Post-quantum hybrid mesh handshake

## Status
Accepted

## Context
Classical ed25519 receipts (Phase 4) and QUIC/TLS (Phase 1) are not quantum-resistant. NIST ML-KEM / ML-DSA provide pure-Rust options that build on Windows without C toolchains.

## Decision
Add an **application-layer hybrid envelope** (`cnk::security`):
1. ML-KEM-768 encapsulation → shared secret
2. ML-DSA-65 signature over transcript
3. Lightweight handshake puzzle (leading-zero SHA3) for anti-amplification
4. Peer rate limits + reputation scoring

Transport TLS/QUIC remains classical for Quinn today. Document migration path: when stacks support hybrid KEM in TLS 1.3, fold CNK secrets into that.

## Consequences
- Works on Windows now.
- Not a full TLS replacement — envelope authenticity + PQ shared secret binding.
- Puzzle difficulty capped for demos (≤16 bits).
