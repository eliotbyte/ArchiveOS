from __future__ import annotations

from typing import Any

from .ytdlp_runner import YtdlpError, probe_url


def discover(url: str, *, playlist_max_items: int | None = None, extra_args: list[str] | None = None) -> dict[str, Any]:
    return probe_url(url, playlist_max_items=playlist_max_items, extra_args=extra_args)


def list_video_ids(probe: dict[str, Any]) -> list[str]:
    if probe.get("_type") == "playlist" or probe.get("entries"):
        entries = probe.get("entries") or []
        return [entry["id"] for entry in entries if entry and entry.get("id")]
    if probe.get("id"):
        return [probe["id"]]
    return []


def is_playlist(probe: dict[str, Any]) -> bool:
    return probe.get("_type") == "playlist" or bool(probe.get("entries"))


def require_entries(probe: dict[str, Any]) -> list[str]:
    video_ids = list_video_ids(probe)
    if not video_ids:
        raise YtdlpError("no entries found for input")
    return video_ids
