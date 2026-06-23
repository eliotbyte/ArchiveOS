from __future__ import annotations

from typing import Any
from urllib.parse import urlparse


def normalize_extractor_key(raw: str | None) -> str | None:
    if not raw:
        return None
    key = raw.strip()
    if key.endswith("IE"):
        key = key[:-2]
    key = key.lower()
    return key or None


def canonical_platform_source(raw: str) -> str:
    """Map yt-dlp extractor variants to stable platform ids used in source_ref."""
    key = raw.lower().strip()
    if key.startswith("youtube"):
        return "youtube"
    return key


def extractor_from_info(info: dict[str, Any] | None) -> str:
    if not info:
        return "unknown"
    for field in ("extractor_key", "extractor", "ie_key"):
        if normalized := normalize_extractor_key(info.get(field)):
            return canonical_platform_source(normalized)
    if webpage_url := info.get("webpage_url") or info.get("original_url"):
        return extractor_from_url(str(webpage_url))
    return "unknown"


def extractor_from_probe(probe: dict[str, Any]) -> str:
    source = extractor_from_info(probe)
    if source != "unknown":
        return source
    if webpage_url := probe.get("webpage_url") or probe.get("original_url"):
        return extractor_from_url(str(webpage_url))
    return "unknown"


def extractor_from_url(url: str) -> str:
    host = (urlparse(url).hostname or "").lower()
    if host.startswith("www."):
        host = host[4:]
    if "youtube" in host or host == "youtu.be":
        return "youtube"
    if "pornhub" in host:
        return "pornhub"
    if "vimeo" in host:
        return "vimeo"
    if host:
        return host.split(".")[0]
    return "unknown"


def video_url_from_info(info: dict[str, Any], video_id: str | None = None) -> str | None:
    for field in ("webpage_url", "original_url", "url"):
        if url := info.get(field):
            return str(url)
    if video_id and extractor_from_info(info) == "youtube":
        return f"https://www.youtube.com/watch?v={video_id}"
    return None


def collection_type(source: str, probe: dict[str, Any], input_url: str) -> str:
    if is_channel_uploads(probe, input_url):
        return f"{source}_channel_uploads"
    return f"{source}_playlist"


def is_channel_uploads(probe: dict[str, Any], input_url: str) -> bool:
    if probe.get("channel_id") and probe.get("id") == probe.get("channel_id"):
        return True
    url = (probe.get("webpage_url") or input_url).lower()
    return "/channel/" in url or "/@" in url or "/users/" in url or "/model/" in url or "/pornstar/" in url
