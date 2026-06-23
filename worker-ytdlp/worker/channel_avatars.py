from __future__ import annotations

from pathlib import Path
from typing import Any

from .download import DownloadError, probe_video
from .manifest_builder import relative_staging_path
from .thumbnails import download_remote_image, extension_from_url, thumbnail_http_headers


def resolve_author_probe_url(info: dict[str, Any]) -> str | None:
    return info.get("channel_url") or info.get("uploader_url")


def best_channel_avatar_url(probe: dict[str, Any]) -> str | None:
    thumbnails = probe.get("thumbnails") or []
    for thumb in thumbnails:
        if thumb.get("id") == "avatar_uncropped":
            url = thumb.get("url")
            if url:
                return url
    for thumb in reversed(thumbnails):
        url = thumb.get("url")
        if not url:
            continue
        width = thumb.get("width") or 0
        height = thumb.get("height") or 0
        if width and height and abs(width - height) <= max(width, height) * 0.15:
            return url
    if thumbnails:
        return thumbnails[-1].get("url")
    return probe.get("thumbnail")


def probe_author_page(url: str, *, extra_args: list[str] | None = None) -> dict[str, Any]:
    args = ["--playlist-items", "0", *(extra_args or [])]
    return probe_video(url, extra_args=args)


def try_channel_avatar(
    channel: dict[str, Any],
    info: dict[str, Any],
    files_dir: Path,
    *,
    extra_args: list[str] | None = None,
) -> dict[str, str] | None:
    external_id = channel.get("external_id")
    if not external_id:
        return None

    author_url = channel.get("url") or resolve_author_probe_url(info)
    if not author_url:
        return None

    try:
        probe = probe_author_page(author_url, extra_args=extra_args)
    except DownloadError:
        return None

    avatar_url = best_channel_avatar_url(probe)
    if not avatar_url:
        return None

    files_dir.mkdir(parents=True, exist_ok=True)
    ext = extension_from_url(avatar_url)
    output_path = files_dir / f"{external_id}{ext}"
    headers = thumbnail_http_headers(probe)
    error = download_remote_image(avatar_url, output_path, headers=headers)
    if error:
        return None

    return {
        "path": relative_staging_path(files_dir, output_path),
        "source_url": avatar_url,
    }
