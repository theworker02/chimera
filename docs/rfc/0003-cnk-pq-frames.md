# RFC-0003: CNK frames & PQ envelope

## Mesh frame
Postcard `MeshFrame { header: FrameHeader, body }` with length-prefixed datagrams for smoltcp/UDP.

| msg_type | Meaning |
|---|---|
| 1 | HEARTBEAT |
| 2 | TX_LOG_SYNC |
| 3 | PQ_HANDSHAKE |
| 4 | TASK |

## PQ handshake messages
1. `HybridHello` — KEM EK, DSA VK, nonce, puzzle challenge  
2. `HybridReply` — KEM CT, DSA VK, nonce, puzzle response, DSA signature, transcript hash  
3. Both parties derive `SHA3-256(ss || transcript)` as session key material

## TxLog sync
Neighbor replicas exchange encoded `TxEntry` lists; receivers `merge_replica` only contiguous extensions of the local tip (no fork choice yet — scaffolding).
