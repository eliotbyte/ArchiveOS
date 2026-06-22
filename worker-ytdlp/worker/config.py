from __future__ import annotations

import os
from dataclasses import dataclass
from pathlib import Path

from .paths import (
    default_youtube_cookies_path,
    ensure_worker_dirs,
    ytdlp_cache_dir,
    ytdlp_cookies_dir,
    ytdlp_worker_dir,
)


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
    ytdlp_cookies_path: str | None
    ytdlp_worker_dir: str
    ytdlp_cache_dir: str
    ytdlp_cookies_dir: str

    @classmethod
    def from_env(cls) -> Config:
        max_items_raw = os.environ.get("YTDLP_PLAYLIST_MAX_ITEMS")  # optional dev cap; unset = full playlist
        vault_path = os.environ["VAULT_PATH"]
        ensure_worker_dirs(vault_path)

        cookies = os.environ.get("YTDLP_COOKIES_PATH")
        if not cookies:
            default = default_youtube_cookies_path(vault_path)
            if default.is_file():
                cookies = str(default)

        return cls(
            core_url=os.environ.get("CORE_URL", "http://core:8080").rstrip("/"),
            vault_name=os.environ["VAULT_NAME"],
            vault_path=vault_path,
            job_poll_secs=int(os.environ.get("JOB_POLL_SECS", "5")),
            job_lease_secs=int(os.environ.get("JOB_LEASE_SECS", "600")),
            ytdlp_update_on_start=_truthy(os.environ.get("YTDLP_UPDATE_ON_START", "1")),
            ytdlp_auto_update=_truthy(os.environ.get("YTDLP_AUTO_UPDATE", "1")),
            ytdlp_update_interval_secs=int(
                os.environ.get("YTDLP_UPDATE_INTERVAL_SECS", "86400")
            ),
            ytdlp_playlist_max_items=int(max_items_raw) if max_items_raw else None,
            ytdlp_cookies_path=cookies if cookies else None,
            ytdlp_worker_dir=str(ytdlp_worker_dir(vault_path)),
            ytdlp_cache_dir=str(ytdlp_cache_dir(vault_path)),
            ytdlp_cookies_dir=str(ytdlp_cookies_dir(vault_path)),
        )


def _truthy(value: str) -> bool:
    return value.lower() not in {"0", "false", "off", "no"}
