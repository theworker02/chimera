# ADR-0003: Content-addressed chunking (ChimeraFS)

## Status
Accepted

## Context
Large datasets must stream across peers with integrity and cache reuse.

## Decision
Slice assets into BLAKE3-addressed blocks, form a Merkle DAG per asset, advertise holders via a gossip-indexed DHT, and expose a **VirtualMount** VFS (FUSE optional on Unix).

## Consequences
- Verify-on-ingest, trust-in-cache thereafter.
- LRU RAM cache + mmap-backed disk blocks.
- Prefetch hooks warm dependencies before Wasm starts.
- Windows builds do not require FUSE.
