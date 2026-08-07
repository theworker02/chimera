# RFC-0001: Chimera Wire Protocol

## Framing
All TCP/QUIC payloads are length-prefixed little-endian `u32` + postcard body.

```
| len:u32 LE | postcard(WireMsg) |
```

## Message classes
| Class | Examples | Priority |
|---|---|---|
| Control | Heartbeat, Reclaim, PageOwn, AgentVote, DhtPeers | Highest |
| Compute | Steal*, Task*, Intent*, Receipt* | Medium |
| Bulk | Block*, PageData/Fetch, MigrateChunk | Lowest (must not starve control) |

## Gossip handshake
1. UDP multicast `GossipAnnounce { peer, known_peers }` on `239.255.74.10:7410` (configurable).
2. Receivers upsert `PeerTable`, remember TCP/QUIC endpoints.
3. Heartbeats over QUIC/TCP refresh caps + `AgentDigest`.
4. Peers missing `heartbeat_ms * heartbeat_misses` are pruned; tasks reclaimed.

## Schema source
Canonical types live in `src/protocol.rs` (`WireMsg`, `TaskSlice`, `ComputeReceipt`, …).
