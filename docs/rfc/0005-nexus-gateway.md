# RFC-0005: Nexus Core function & storage wire surfaces

## Abstract
Phase 9 HTTP surfaces for function gateway and Raft KV (management API port, default `7600`).

## Deploy
`POST /v1/functions`  
```json
{ "tenant": "demo", "name": "add1", "wasm_hex": "...", "memory_mib": 16, "fuel": 5000000 }
```

## Invoke
`POST /v1/functions/invoke`  
```json
{ "tenant": "demo", "function": "add1", "input_hex": "29", "priority": 1 }
```
Priority ≥ 200 is treated as real-time lane traffic (never shed under CPU saturation).

## Scale / logs
- `POST /v1/functions/scale` `{ "tenant", "name", "instances" }`
- `GET /v1/functions/logs`

## KV
- `POST /v1/kv` `{ "key", "value_hex" }`
- `GET /v1/kv/{key}`

## Auth
`Authorization: Bearer role:name` (e.g. `admin:ops`) — same as Phase 7.

## Roadmap (non-normative)
gRPC invoke, event triggers, container image ingest, SQL-over-KV.
