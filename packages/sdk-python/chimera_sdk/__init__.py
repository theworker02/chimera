"""Chimera Python SDK — thin async client for the management REST API."""

from __future__ import annotations

from typing import Any, Optional

import httpx


class ChimeraClient:
    def __init__(
        self,
        base_url: str = "http://127.0.0.1:7600",
        auth: str = "admin:ops",
        timeout: float = 30.0,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.auth = auth
        self._client = httpx.AsyncClient(
            base_url=self.base_url,
            headers={"Authorization": f"Bearer {auth}"},
            timeout=timeout,
        )

    async def aclose(self) -> None:
        await self._client.aclose()

    async def __aenter__(self) -> "ChimeraClient":
        return self

    async def __aexit__(self, *args: Any) -> None:
        await self.aclose()

    async def health(self) -> dict[str, Any]:
        r = await self._client.get("/health")
        r.raise_for_status()
        return r.json()

    async def cluster(self) -> dict[str, Any]:
        r = await self._client.get("/v1/cluster")
        r.raise_for_status()
        return r.json()

    async def submit_intent(self, declaration: str) -> dict[str, Any]:
        r = await self._client.post("/v1/intents", json={"declaration": declaration})
        r.raise_for_status()
        return r.json()

    async def list_intents(self) -> list[dict[str, Any]]:
        r = await self._client.get("/v1/intents")
        r.raise_for_status()
        return r.json()

    async def pin_asset(self, name: str, data: bytes) -> dict[str, Any]:
        r = await self._client.post(
            "/v1/assets",
            json={"name": name, "data_hex": data.hex()},
        )
        r.raise_for_status()
        return r.json()

    async def list_assets(self) -> list[dict[str, Any]]:
        r = await self._client.get("/v1/assets")
        r.raise_for_status()
        return r.json()

    async def get_asset(self, name: str) -> dict[str, Any]:
        r = await self._client.get(f"/v1/assets/{name}")
        r.raise_for_status()
        return r.json()

    async def issue_token(
        self,
        role: str = "operator",
        ttl_secs: int = 3600,
        node_hint: Optional[str] = None,
    ) -> dict[str, Any]:
        body: dict[str, Any] = {"role": role, "ttl_secs": ttl_secs}
        if node_hint:
            body["node_hint"] = node_hint
        r = await self._client.post("/v1/tokens", json=body)
        r.raise_for_status()
        return r.json()

    async def metrics_text(self) -> str:
        r = await self._client.get("/metrics")
        r.raise_for_status()
        return r.text
