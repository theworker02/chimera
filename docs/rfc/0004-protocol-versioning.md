# RFC-0004: Wire protocol versioning & rolling upgrades

## Versions
- Current: **major=1 minor=1** (`WIRE_MAJOR` / `WIRE_MINOR`)
- Minimum supported major: **1** (`WIRE_MIN_MAJOR`)

## Negotiation
`WireMsg::ProtocolHello { from, version }` carries `ProtocolVersion { major, minor, min_major }`.
Peers select `major = min(local.major, peer.major)` if each major ≥ peer.min_major; else disconnect.

## Rolling-upgrade rules
1. Never remove postcard fields in a minor bump — only add optional trailing fields.
2. Major bump required for: renamed enums, changed endianness, removed message variants.
3. New nodes must keep `min_major` ≤ oldest fleet major until drained.
4. Management API `/v1/protocol` exposes local version for out-of-band checks.

## Compatibility matrix (1.x)
| Local | Peer | Result |
|---|---|---|
| 1.1 | 1.0 | OK @ 1.0 |
| 1.1 | 1.1 | OK @ 1.1 |
| 2.0 (min 2) | 1.1 | Reject |
