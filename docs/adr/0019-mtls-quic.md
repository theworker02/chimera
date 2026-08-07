# ADR-0019: mTLS over QUIC

## Status
Accepted (Phase 11)

## Decision
- Lab `LocalCa` mints client+server leaves; `mtls_server_endpoint` requires client certs via rustls `WebPkiClientVerifier`.
- Default mesh transport remains skip-verify + no client auth for LAN demos (backward compatible).
- mTLS path is tested in-process: authenticated handshake succeeds; unauthenticated peer is rejected.

## Status labels
| Surface | Status |
|---|---|
| mTLS helpers + unit test | **working** |
| Default gossip mesh QUIC | **working** (no mTLS by default) |
| Production PKI / HSM | **roadmap** |
