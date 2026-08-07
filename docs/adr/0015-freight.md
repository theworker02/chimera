# ADR-0015: Freight decentralized package registry

## Status
Accepted (Phase 10)

## Context
WorldOS needs a P2P app store without a central registry.

## Decision
- Packages are Wasm modules addressed by BLAKE3, described by a signed `PackageManifest` (name, version, hash, ed25519 publisher key, deps).
- Publish stores the module in ChimeraFS CAS and indexes the manifest in a local Freight registry (DHT announce via CAS block providers).
- Install verifies signature + hash, then deploys into the Nexus function gateway.
- Trust model: **signature-based**. There is no central authority; users must trust publisher public keys. Censorship-resistance means anyone can republish signed packages — it does **not** mean anonymous or untraceable distribution.

## Consequences
`chimeractl freight publish|search|install|run` and MeshShell Freight panel are the UX. Container packages remain roadmap.
