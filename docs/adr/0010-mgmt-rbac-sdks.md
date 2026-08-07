# ADR-0010: Management API, RBAC, and SDKs

## Status
Accepted

## Context
Operators need declarative control of intents, assets, join tokens, and audit without SSH into nodes.

## Decision
- Axum REST API under `/v1/*` + embedded portal `/` + `/health` + `/metrics`.
- RBAC roles: admin / operator / submitter / reader via `Authorization: Bearer role:name`.
- Tamper-evident audit JSONL (BLAKE3 chain + ed25519).
- `chimeractl` CLI; Python (`httpx`) and TypeScript (`fetch`) SDKs.
- gRPC (tonic) deferred — document as future; REST is the supported surface.
- WIT / component-model native bindings: future path (see ADR-0012 for Nexus).

## Consequences
Auth is demo-grade bearer roles (replace with OIDC/mTLS in production).
