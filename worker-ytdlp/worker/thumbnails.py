from __future__ import annotations

import urllib.request
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

ALLOWED_EXTENSIONS = {".jpg", ".jpeg", ".png", ".webp"}

_WIDESCREEN_AR = 16 / 9
_LEGACY_AR = 4 / 3
_AR_TOLERANCE = 0.06

_YOUTUBE_FALLBACK_SUFFIXES = (
    "maxresdefault.jpg",
    "mqdefault.jpg",
    "hqdefault.jpg",
    "sddefault.jpg",
    "default.jpg",
)

_URL_QUALITY_HINTS = (
    ("maxres", 5),
    ("sddefault", 4),
    ("hqdefault", 3),
    ("mqdefault", 2),
    ("default", 1),
)


class ThumbnailError(RuntimeError):
    pass


def thumbnail_external_id(video_id: str) -> str:
    return f"{video_id}:thumbnail"


def best_thumbnail_url(info: dict[str, Any]) -> str | None:
    candidates = thumbnail_url_candidates(info, str(info.get("id") or ""))
    return candidates[0] if candidates else None


def _entry_resolution(entry: dict[str, Any]) -> int:
    width = entry.get("width") or 0
    height = entry.get("height") or 0
    try:
        return int(width) * int(height)
    except (TypeError, ValueError):
        return 0


def _url_quality_hint(url: str) -> int:
    lower = url.lower()
    for needle, score in _URL_QUALITY_HINTS:
        if needle in lower:
            return score
    return 0


def _aspect_ratio(entry: dict[str, Any]) -> float | None:
    width = entry.get("width")
    height = entry.get("height")
    try:
        w, h = int(width), int(height)
    except (TypeError, ValueError):
        return None
    if h <= 0:
        return None
    return w / h


def _aspect_bucket(entry: dict[str, Any], url: str = "") -> int:
    """0 = native 16:9 (prefer), 1 = unknown, 2 = legacy 4:3 letterbox (avoid)."""
    ar = _aspect_ratio(entry)
    if ar is not None:
        if abs(ar - _WIDESCREEN_AR) <= _AR_TOLERANCE:
            return 0
        if abs(ar - _LEGACY_AR) <= _AR_TOLERANCE:
            return 2
        return 1

    url_lower = url.lower()
    if any(needle in url_lower for needle in ("maxresdefault", "mqdefault")):
        return 0
    if any(needle in url_lower for needle in ("hqdefault", "sddefault", "/default.")):
        return 2
    return 1


def thumbnail_url_candidates(info: dict[str, Any], video_id: str) -> list[str]:
    entries = [entry for entry in (info.get("thumbnails") or []) if entry and entry.get("url")]
    entries.sort(
        key=lambda entry: (
            _aspect_bucket(entry, str(entry.get("url") or "")),
            -_entry_resolution(entry),
            -_url_quality_hint(str(entry.get("url") or "")),
        ),
    )

    urls: list[str] = [str(entry["url"]) for entry in entries if entry.get("url")]

    thumb = info.get("thumbnail")
    if isinstance(thumb, str) and thumb and thumb not in urls:
        urls.append(thumb)

    if video_id:
        for suffix in _YOUTUBE_FALLBACK_SUFFIXES:
            url = f"https://i.ytimg.com/vi/{video_id}/{suffix}"
            if url not in urls:
                urls.append(url)

    return urls


def thumbnail_http_headers(info: dict[str, Any]) -> dict[str, str]:
    headers = {"User-Agent": "ArchiveOS-ytdlp-worker/1.0"}
    info_headers = info.get("http_headers")
    if isinstance(info_headers, dict):
        headers.update({str(k): str(v) for k, v in info_headers.items() if v is not None})
    return headers


def extension_from_url(url: str) -> str:
    path = urlparse(url).path.lower()
    for ext in ALLOWED_EXTENSIONS:
        if path.endswith(ext):
            return ext if ext != ".jpeg" else ".jpg"
    return ".jpg"


def download_remote_image(
    url: str,
    output_path: Path,
    *,
    headers: dict[str, str] | None = None,
) -> str | None:
    request_headers = {"User-Agent": "ArchiveOS-ytdlp-worker/1.0"}
    if headers:
        request_headers.update(headers)

    import urllib.request

    request = urllib.request.Request(url, headers=request_headers)
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            data = response.read()
    except Exception as err:
        return f"image download failed: {err}"

    if not data:
        return "image download returned empty body"

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_bytes(data)
    return None


def download_thumbnail(
    video_id: str,
    info: dict[str, Any],
    output_dir: Path,
) -> tuple[Path | None, str | None]:
    output_dir.mkdir(parents=True, exist_ok=True)
    headers = thumbnail_http_headers(info)
    last_error: str | None = None

    for url in thumbnail_url_candidates(info, video_id):
        ext = extension_from_url(url)
        output_path = output_dir / f"{video_id}{ext}"
        error = download_remote_image(url, output_path, headers=headers)
        if error is None:
            return output_path, None
        last_error = error

    if last_error:
        return None, last_error
    return None, "no thumbnail URL in metadata"
