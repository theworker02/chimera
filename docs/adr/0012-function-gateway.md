# ADR-0012: Universal Polyglot Function Gateway

## Status
Accepted (Phase 9)

## Context
Chimera needs a multi-tenant function runtime for mesh-wide deploy/invoke. Candidates include containers, language VMs, and Wasm.

## Decision
- **Working backend:** precompiled Wasm modules via Wasmtime, with per-tenant engines, fuel, and memory caps.
- **Deployment pipeline:** abstract store → compile → register → invoke; Wasm is the only complete adapter today.
- **Auth:** Phase 7 RBAC (`SubmitWorkload` to deploy/invoke; `ManageNodes` to scale).
- **Storage of blobs:** in-memory CAS keyed by BLAKE3 (ChimeraFS CAS integration is the distribution path for multi-node).

## Non-goals / roadmap
- Container / Dockerfile ingestion — **roadmap adapter**, not faked.
- Raw Python/JS via Wasm interpreters — only if a clean crate path appears; not shipped.
- gRPC / event triggers — documented roadmap; HTTP REST is the working surface.

## Consequences
chimeractl `deploy`/`invoke` and `/v1/functions*` are production-leaning for Wasm demos. Do not claim container portability in marketing copy.
