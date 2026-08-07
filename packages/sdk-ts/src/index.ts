/**
 * Chimera TypeScript SDK — fetch-based async client for the management REST API.
 */

export type ChimeraAuth = string;

export interface ChimeraClientOptions {
  baseUrl?: string;
  auth?: ChimeraAuth;
  fetchImpl?: typeof fetch;
}

export class ChimeraClient {
  private baseUrl: string;
  private auth: string;
  private fetchImpl: typeof fetch;

  constructor(opts: ChimeraClientOptions = {}) {
    this.baseUrl = (opts.baseUrl ?? "http://127.0.0.1:7600").replace(/\/$/, "");
    this.auth = opts.auth ?? "admin:ops";
    this.fetchImpl = opts.fetchImpl ?? fetch;
  }

  private async req<T>(path: string, init: RequestInit = {}): Promise<T> {
    const headers = new Headers(init.headers);
    headers.set("Authorization", `Bearer ${this.auth}`);
    if (init.body && !headers.has("Content-Type")) {
      headers.set("Content-Type", "application/json");
    }
    const res = await this.fetchImpl(`${this.baseUrl}${path}`, { ...init, headers });
    if (!res.ok) {
      throw new Error(`${res.status} ${await res.text()}`);
    }
    const ct = res.headers.get("content-type") ?? "";
    if (ct.includes("application/json")) {
      return (await res.json()) as T;
    }
    return (await res.text()) as T;
  }

  health() {
    return this.req<Record<string, unknown>>("/health");
  }

  cluster() {
    return this.req<Record<string, unknown>>("/v1/cluster");
  }

  submitIntent(declaration: string) {
    return this.req<Record<string, unknown>>("/v1/intents", {
      method: "POST",
      body: JSON.stringify({ declaration }),
    });
  }

  listIntents() {
    return this.req<unknown[]>("/v1/intents");
  }

  async pinAsset(name: string, data: Uint8Array | Buffer) {
    const hex = Buffer.from(data).toString("hex");
    return this.req<Record<string, unknown>>("/v1/assets", {
      method: "POST",
      body: JSON.stringify({ name, data_hex: hex }),
    });
  }

  listAssets() {
    return this.req<unknown[]>("/v1/assets");
  }

  getAsset(name: string) {
    return this.req<Record<string, unknown>>(`/v1/assets/${encodeURIComponent(name)}`);
  }

  issueToken(role = "operator", ttlSecs = 3600, nodeHint?: string) {
    return this.req<Record<string, unknown>>("/v1/tokens", {
      method: "POST",
      body: JSON.stringify({ role, ttl_secs: ttlSecs, node_hint: nodeHint }),
    });
  }

  async metricsText() {
    return this.req<string>("/metrics");
  }
}
