/**
 * Chimera TypeScript SDK — fetch-based client for the management REST API.
 */

export class ChimeraApiError extends Error {
  readonly status: number;
  readonly payload?: unknown;

  constructor(message: string, status: number, payload?: unknown) {
    super(message);
    this.name = "ChimeraApiError";
    this.status = status;
    this.payload = payload;
  }
}

export type ChimeraAuth = string;

export interface ChimeraClientOptions {
  baseUrl?: string;
  auth?: ChimeraAuth;
  fetchImpl?: typeof fetch;
}

export interface HealthResponse {
  status: string;
  node: string;
  node_id: string;
  wire: string;
  peers: number;
  completed_tasks: number;
  cpu_pct: number;
}

export interface ClusterResponse {
  node: string;
  peers: number;
  pending: number;
  running: number;
  completed: number;
  fs_blocks: number;
  mem_faults: number;
  migrations: number;
  verified_receipts: number;
}

export interface IntentRecord {
  id: string;
  declaration: string;
  status: string;
}

export interface AssetRecord {
  name: string;
  root_hex: string;
  size: number;
}

export interface TokenIssueResult {
  token: string;
  expires_ms: number;
  role: string;
}

export interface JoinVerifyResult {
  ok: boolean;
  role: string;
  mesh_id: string;
}

export interface FunctionDeployInput {
  tenant: string;
  name: string;
  wasm: Uint8Array | string;
  memoryMib?: number;
  fuel?: number;
}

export interface FunctionInvokeInput {
  tenant: string;
  function: string;
  input: Uint8Array | string;
  priority?: number;
}

export interface ScaleInput {
  tenant: string;
  name: string;
  instances: number;
}

export interface FreightPublishInput {
  name: string;
  version: string;
  wasm: Uint8Array | string;
  description?: string;
}

export interface FreightInstallInput {
  name: string;
  version: string;
  tenant?: string;
}

export interface FreightRunInput {
  name: string;
  input: Uint8Array | string;
  tenant?: string;
}

function trimSlash(url: string): string {
  return url.replace(/\/+$/, "");
}

export function toHex(data: Uint8Array | string): string {
  if (typeof data === "string") return data;
  return Array.from(data, (b) => b.toString(16).padStart(2, "0")).join("");
}

/**
 * Typed client for Chimera's local management API (`http://127.0.0.1:7600` by default).
 * Auth is `Authorization: Bearer role:name` (demo) or an issued token.
 */
export class ChimeraClient {
  private baseUrl: string;
  private auth: string;
  private fetchImpl: typeof fetch;

  constructor(opts: ChimeraClientOptions = {}) {
    this.baseUrl = trimSlash(opts.baseUrl ?? "http://127.0.0.1:7600");
    this.auth = opts.auth ?? "admin:ops";
    this.fetchImpl = opts.fetchImpl ?? fetch;
  }

  setAuth(auth: ChimeraAuth): void {
    this.auth = auth;
  }

  health() {
    return this.req<HealthResponse>("/health");
  }

  metricsText() {
    return this.req<string>("/metrics");
  }

  cluster() {
    return this.req<ClusterResponse>("/v1/cluster");
  }

  protocol() {
    return this.req<Record<string, unknown>>("/v1/protocol");
  }

  submitIntent(declaration: string) {
    return this.req<IntentRecord>("/v1/intents", {
      method: "POST",
      body: JSON.stringify({ declaration }),
    });
  }

  listIntents() {
    return this.req<IntentRecord[]>("/v1/intents");
  }

  pinAsset(name: string, data: Uint8Array | string) {
    return this.req<AssetRecord>("/v1/assets", {
      method: "POST",
      body: JSON.stringify({ name, data_hex: toHex(data) }),
    });
  }

  listAssets() {
    return this.req<AssetRecord[]>("/v1/assets");
  }

  getAsset(name: string) {
    return this.req<AssetRecord>(`/v1/assets/${encodeURIComponent(name)}`);
  }

  issueToken(role = "operator", ttlSecs = 3600, nodeHint?: string) {
    return this.req<TokenIssueResult>("/v1/tokens", {
      method: "POST",
      body: JSON.stringify({ role, ttl_secs: ttlSecs, node_hint: nodeHint }),
    });
  }

  verifyJoin(token: string) {
    return this.req<JoinVerifyResult>("/v1/join/verify", {
      method: "POST",
      body: JSON.stringify({ token }),
    });
  }

  audit() {
    return this.req<{ path: string; entries: number }>("/v1/audit");
  }

  listFunctions(tenant = "demo") {
    const query = new URLSearchParams({ tenant });
    return this.req<unknown[]>(`/v1/functions?${query}`);
  }

  deployFunction(input: FunctionDeployInput) {
    return this.req<Record<string, unknown>>("/v1/functions", {
      method: "POST",
      body: JSON.stringify({
        tenant: input.tenant,
        name: input.name,
        wasm_hex: toHex(input.wasm),
        memory_mib: input.memoryMib,
        fuel: input.fuel,
      }),
    });
  }

  invokeFunction(input: FunctionInvokeInput) {
    return this.req<{
      output_hex: string;
      fuel_used: number;
      peer: string;
      duration_ms: number;
    }>("/v1/functions/invoke", {
      method: "POST",
      body: JSON.stringify({
        tenant: input.tenant,
        function: input.function,
        input_hex: toHex(input.input),
        priority: input.priority ?? 0,
      }),
    });
  }

  scaleFunction(input: ScaleInput) {
    return this.req<{ ok: boolean; instances: number }>("/v1/functions/scale", {
      method: "POST",
      body: JSON.stringify(input),
    });
  }

  functionLogs() {
    return this.req<{ lines: string[] }>("/v1/functions/logs");
  }

  getKv(key: string) {
    return this.req<{ key: string; value_hex: string }>(`/v1/kv/${encodeURIComponent(key)}`);
  }

  setKv(key: string, value: Uint8Array | string) {
    return this.req<{ ok: boolean; key: string }>("/v1/kv", {
      method: "POST",
      body: JSON.stringify({ key, value_hex: toHex(value) }),
    });
  }

  listFs() {
    return this.req<{ items: AssetRecord[] }>("/v1/fs");
  }

  uploadFs(name: string, data: Uint8Array | string) {
    return this.req<AssetRecord>("/v1/fs/upload", {
      method: "POST",
      body: JSON.stringify({ name, data_hex: toHex(data) }),
    });
  }

  getFsByHash(hash: string) {
    return this.req<AssetRecord & { data_hex: string }>(
      `/v1/fs/by-hash/${encodeURIComponent(hash)}`,
    );
  }

  searchFreight(q = "") {
    const query = new URLSearchParams({ q });
    return this.req<{ items: unknown[] }>(`/v1/freight/search?${query}`);
  }

  publishFreight(input: FreightPublishInput) {
    return this.req<Record<string, unknown>>("/v1/freight/publish", {
      method: "POST",
      body: JSON.stringify({
        name: input.name,
        version: input.version,
        wasm_hex: toHex(input.wasm),
        description: input.description,
      }),
    });
  }

  publishFreightDemo() {
    return this.req<Record<string, unknown>>("/v1/freight/publish-demo", { method: "POST" });
  }

  installFreight(input: FreightInstallInput) {
    return this.req<Record<string, unknown>>("/v1/freight/install", {
      method: "POST",
      body: JSON.stringify(input),
    });
  }

  runFreight(input: FreightRunInput) {
    return this.req<Record<string, unknown>>("/v1/freight/run", {
      method: "POST",
      body: JSON.stringify({
        name: input.name,
        tenant: input.tenant,
        input_hex: toHex(input.input),
      }),
    });
  }

  ledgerBalance(account: string) {
    return this.req<{ account: string; balance: number; bypass: boolean }>(
      `/v1/ledger/${encodeURIComponent(account)}`,
    );
  }

  creditLedger(account: string, amount: number) {
    return this.req<{ account: string; balance: number }>("/v1/ledger/credit", {
      method: "POST",
      body: JSON.stringify({ account, amount }),
    });
  }

  private async req<T>(path: string, init: RequestInit = {}): Promise<T> {
    const headers = new Headers(init.headers);
    headers.set("Authorization", `Bearer ${this.auth}`);
    if (init.body && !headers.has("Content-Type")) {
      headers.set("Content-Type", "application/json");
    }
    const res = await this.fetchImpl(`${this.baseUrl}${path}`, { ...init, headers });
    const text = await res.text();
    if (!res.ok) {
      throw new ChimeraApiError(`${res.status} ${path}`, res.status, text);
    }
    const ct = res.headers.get("content-type") ?? "";
    if (ct.includes("application/json")) {
      return JSON.parse(text) as T;
    }
    return text as T;
  }
}

export default ChimeraClient;
