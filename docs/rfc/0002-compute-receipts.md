# RFC-0002: Verifiable Compute Receipts

## Default path (always on)
A receipt binds:
- `task_id`, `executor` node id
- `transcript_hash` (BLAKE3 of Wasm output)
- `fuel_consumed`
- `io_merkle_root` (BLAKE3 of input buffer)
- ed25519 `public_key` + `signature` over the concatenated preimage
- `timestamp_ms`

Verification: recompute preimage, `VerifyingKey::verify`.

Requesters **must** verify before accepting state mutations / completing jobs.

## Optional ZK path
`--features zk-receipts` enables stub prove/verify hooks for future arkworks/bellman circuits. Default Windows builds do **not** pull ZK dependencies.
