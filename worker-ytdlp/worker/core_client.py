from __future__ import annotations

from typing import Any

import requests


class CoreClient:
    def __init__(self, core_url: str, vault_name: str) -> None:
        self.core_url = core_url.rstrip("/")
        self.vault_name = vault_name
        self.session = requests.Session()

    def claim_job(self, job_type: str, lease_secs: int) -> dict[str, Any] | None:
        response = self.session.post(
            f"{self.core_url}/vaults/{self.vault_name}/jobs/claim",
            json={"type": job_type, "lease_secs": lease_secs},
            timeout=30,
        )
        response.raise_for_status()
        return response.json()

    def heartbeat(self, job_id: str, lease_secs: int) -> None:
        response = self.session.post(
            f"{self.core_url}/vaults/{self.vault_name}/jobs/{job_id}/heartbeat",
            json={"lease_secs": lease_secs},
            timeout=30,
        )
        response.raise_for_status()

    def sources_has(
        self,
        external_ids: list[str],
        *,
        source: str = "youtube",
        kind: str = "video",
    ) -> dict[str, bool]:
        if not external_ids:
            return {}
        response = self.session.get(
            f"{self.core_url}/vaults/{self.vault_name}/sources/has",
            params={
                "source": source,
                "kind": kind,
                "ids": ",".join(external_ids),
            },
            timeout=30,
        )
        response.raise_for_status()
        hits = response.json()
        present: dict[str, bool] = {}
        for hit in hits:
            present[hit["external_id"]] = bool(hit.get("present"))
        return present

    def submit_manifest(
        self,
        job_id: str,
        manifest: dict[str, Any],
        *,
        status: str | None = None,
    ) -> dict[str, Any]:
        payload = dict(manifest)
        if status:
            payload["status"] = status
        response = self.session.post(
            f"{self.core_url}/vaults/{self.vault_name}/jobs/{job_id}/manifest",
            json=payload,
            timeout=120,
        )
        response.raise_for_status()
        return response.json()

    def create_job(self, job_type: str, input_url: str) -> dict[str, Any]:
        response = self.session.post(
            f"{self.core_url}/vaults/{self.vault_name}/jobs",
            json={"type": job_type, "input": input_url},
            timeout=30,
        )
        response.raise_for_status()
        return response.json()

    def record_failure(
        self,
        *,
        job_id: str | None,
        source: str,
        kind: str,
        external_id: str,
        url: str | None,
        stage: str,
        error_kind: str,
        message: str,
        retryable: bool,
    ) -> dict[str, Any]:
        payload = {
            "job_id": job_id,
            "source": source,
            "kind": kind,
            "external_id": external_id,
            "url": url,
            "stage": stage,
            "error_kind": error_kind,
            "message": message,
            "retryable": retryable,
        }
        response = self.session.post(
            f"{self.core_url}/vaults/{self.vault_name}/source-failures",
            json=payload,
            timeout=30,
        )
        response.raise_for_status()
        return response.json()
