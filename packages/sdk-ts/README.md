# `@theworker02/chimera-sdk`

Typed control client for Chimera's management REST API (`http://127.0.0.1:7600` by default).

Auth is `Authorization: Bearer role:name` (demo principal) or a token from `issueToken()`.

```ts
import { ChimeraClient } from "@theworker02/chimera-sdk";

const api = new ChimeraClient({ auth: "admin:ops" });
await api.health();
const intent = await api.submitIntent("run add1");
await api.deployFunction({
  tenant: "demo",
  name: "add1",
  wasm: wasmBytes,
});
const out = await api.invokeFunction({
  tenant: "demo",
  function: "add1",
  input: "00",
});
```

```bash
npm install
npm run typecheck
```

## Coverage

Health, metrics, cluster, protocol, intents, assets, tokens, join verify, audit, functions (list/deploy/invoke/scale/logs), KV, FS, Freight, and ledger.
