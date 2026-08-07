# ADR-0018: TEE attestation abstraction

## Status
Accepted (Phase 11)

## Decision
- `TeeProvider` trait with `TeeAttestation` (measurement, nonce, quote, pubkey).
- **SimulatedTee** — BLAKE3 image measurement + ed25519 quote. **Status: working**
- Hardware stubs: Intel TDX, AMD SEV-SNP, ARM TrustZone — return explicit unimplemented errors. **Status: roadmap**
- Attestation can ride in `AttestedHandshake` alongside PQ material (Phase 6).

## Honesty
No real enclave hardware is exercised in CI. Do not claim government-accredited TEE without the matching backend + certification.
