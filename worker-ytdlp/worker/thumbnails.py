from __future__ import annotations

import urllib.request
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

ALLOWED_EXTENSIONS = {".jpg", ".jpeg", ".png", ".webp"}


class ThumbnailError(RuntimeError):
    pass


def thumbnail_external_id(video_id: str) -> str:
    return f"{video_id}:thumbnail"


def best_thumbnail_url(info: dict[str, Any]) -> str | None:
    thumbnails = info.get("thumbnails") or []
    if thumbnails:
        url = thumbnails[-1].get("url")
        if url:
            return url
    return info.get("thumbnail")


def extension_from_url(url: str) -> str:
    path = urlparse(url).path.lower()
    for ext in ALLOWED_EXTENSIONS:
        if path.endswith(ext):
            return ext if ext != ".jpeg" else ".jpg"
    return ".jpg"


def download_thumbnail(
    video_id: str,
    info: dict[str, Any],
    output_dir: Path,
) -> tuple[Path | None, str | None]:
    url = best_thumbnail_url(info)
    if not url:
        return None, "no thumbnail URL in metadata"

    output_dir.mkdir(parents=True, exist_ok=True)
    ext = extension_from_url(url)
    output_path = output_dir / f"{video_id}{ext}"

    request = urllib.request.Request(
        url,
        headers={"User-Agent": "ArchiveOS-ytdlp-worker/1.0"},
    )
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            data = response.read()
    except Exception as err:
        return None, f"thumbnail download failed: {err}"

    if not data:
        return None, "thumbnail download returned empty body"

    output_path.write_bytes(data)
    return output_path, None
