from __future__ import annotations

import json
import logging
import subprocess
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

import requests

logger = logging.getLogger(__name__)


class TrackDownloadError(Exception):
    pass


def _metadata_map(asset: dict[str, Any]) -> dict[str, str]:
    metadata = asset.get("metadata") or {}
    if isinstance(metadata, dict):
        return {str(k): str(v) for k, v in metadata.items() if v is not None}
    return {}


def _parse_http_headers(raw: str | None) -> dict[str, str]:
    if not raw:
        return {}
    try:
        parsed = json.loads(raw)
    except json.JSONDecodeError:
        return {}
    if not isinstance(parsed, dict):
        return {}
    return {str(k): str(v) for k, v in parsed.items()}


def _output_ext(asset: dict[str, Any], metadata: dict[str, str]) -> str:
    ext = metadata.get("ext") or asset.get("ext") or "bin"
    ext = ext.lstrip(".")
    return ext or "bin"


def download_subtitle_asset(asset: dict[str, Any], files_dir: Path) -> Path:
    metadata = _metadata_map(asset)
    source_url = metadata.get("source_url")
    if not source_url:
        raise TrackDownloadError("subtitle asset missing metadata.source_url")

    headers = _parse_http_headers(metadata.get("http_headers"))
    response = requests.get(source_url, headers=headers or None, timeout=120)
    response.raise_for_status()

    ext = _output_ext(asset, metadata)
    out_path = files_dir / f"{asset['id']}.{ext}"
    out_path.write_bytes(response.content)
    return out_path


def download_audio_asset(asset: dict[str, Any], files_dir: Path) -> Path:
    metadata = _metadata_map(asset)
    format_id = metadata.get("format_id")
    source_page_url = metadata.get("source_page_url")
    if not format_id:
        raise TrackDownloadError("audio asset missing metadata.format_id")
    if not source_page_url:
        raise TrackDownloadError("audio asset missing metadata.source_page_url")

    asset_id = asset["id"]
    output_template = str(files_dir / f"{asset_id}.%(ext)s")
    cmd = [
        "yt-dlp",
        "-f",
        format_id,
        "--no-playlist",
        "-o",
        output_template,
        source_page_url,
    ]
    logger.info("audio download: %s", " ".join(cmd))
    result = subprocess.run(cmd, capture_output=True, text=True, check=False)
    if result.returncode != 0:
        detail = (result.stderr or result.stdout or "yt-dlp failed").strip()
        raise TrackDownloadError(detail)

    matches = sorted(files_dir.glob(f"{asset_id}.*"))
    if not matches:
        raise TrackDownloadError(f"yt-dlp produced no output for asset {asset_id}")
    return matches[0]


def download_track_asset(asset: dict[str, Any], files_dir: Path) -> Path:
    kind = asset.get("kind")
    files_dir.mkdir(parents=True, exist_ok=True)
    if kind == "subtitle":
        return download_subtitle_asset(asset, files_dir)
    if kind == "audio":
        return download_audio_asset(asset, files_dir)
    raise TrackDownloadError(f"unsupported asset kind for track download: {kind!r}")


def relative_staging_path(files_dir: Path, file_path: Path) -> str:
    staging_dir = files_dir.parent
    rel = file_path.relative_to(staging_dir)
    return rel.as_posix()


def source_failure_url(asset: dict[str, Any]) -> str | None:
    metadata = _metadata_map(asset)
    for key in ("source_url", "source_page_url"):
        value = metadata.get(key)
        if value and urlparse(value).scheme:
            return value
    return None
