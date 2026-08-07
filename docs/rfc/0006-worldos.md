# RFC-0006: WorldOS MeshShell & Freight surfaces

## MeshShell
Browser SPA at `GET /meshshell` (static HTML/JS/CSS under `meshshell/web/`, embedded in the node binary).

| Route | Purpose |
|---|---|
| `GET /v1/fs` | List ChimeraFS mounts |
| `POST /v1/fs/upload` | `{ name, data_hex }` → CAS ingest |
| `GET /v1/fs/by-hash/{hash}` | Download asset bytes |
| `GET /v1/collab/ws?session=` | Collaborative notes WebSocket |

Native GUI is **roadmap**; desktop control remains TUI + chimeractl.

## Freight
| Route | Purpose |
|---|---|
| `POST /v1/freight/publish` | Sign+store package |
| `GET /v1/freight/search?q=` | Discover |
| `POST /v1/freight/install` | Verify + gateway deploy |
| `POST /v1/freight/run` | Invoke installed package |

## Ledger
| Route | Purpose |
|---|---|
| `GET /v1/ledger/{account}` | Balance |
| `POST /v1/ledger/credit` | Operator top-up |
