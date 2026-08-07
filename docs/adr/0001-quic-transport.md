# ADR-0001: QUIC transport for the Chimera mesh

## Status
Accepted

## Context
Nodes must exchange control frames (heartbeats, reclaim, ownership) and bulk payloads (CAS blocks, DSM pages, Wasm snapshots) without a central broker. TCP alone stalls control under bulk load; HTTP overlays add latency.

## Decision
Use **Quinn/QUIC** as the primary mesh transport with **TCP framed postcard** as a reliable fallback. Classify streams as Control / Compute / Bulk so heartbeats are never starved by asset streaming.

## Consequences
- Self-signed TLS certs via `rcgen` for LAN/mesh demos (replace with pinned PKI for production).
- ALPN `chimera`.
- Postcard length-prefixed frames on both transports.
