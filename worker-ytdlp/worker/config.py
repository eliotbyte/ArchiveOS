from __future__ import annotations

import os
from dataclasses import dataclass


@dataclass(frozen=True)
class Config:
    core_url: str
    vault_name: str
    vault_path: str
    job_poll_secs: int
    job_lease_secs: int
    ytdlp_update_on_start: bool
    ytdlp_auto_update: bool
    ytdlp_update_interval_secs: int
    ytdlp_playlist_max_items: int | None

    @classmethod
    def from_env(cls) -> Config:
        max_items_raw = os.environ.get("YTDLP_PLAYLIST_MAX_ITEMS")
        return cls(
            core_url=os.environ.get("CORE_URL", "http://core:8080").rstrip("/"),
            vault_name=os.environ["VAULT_NAME"],
            vault_path=os.environ["VAULT_PATH"],
            job_poll_secs=int(os.environ.get("JOB_POLL_SECS", "5")),
            job_lease_secs=int(os.environ.get("JOB_LEASE_SECS", "600")),
            ytdlp_update_on_start=_truthy(os.environ.get("YTDLP_UPDATE_ON_START", "1")),
            ytdlp_auto_update=_truthy(os.environ.get("YTDLP_AUTO_UPDATE", "1")),
            ytdlp_update_interval_secs=int(
                os.environ.get("YTDLP_UPDATE_INTERVAL_SECS", "86400")
            ),
            ytdlp_playlist_max_items=int(max_items_raw) if max_items_raw else None,
        )


def _truthy(value: str) -> bool:
    return value.lower() not in {"0", "false", "off", "no"}
