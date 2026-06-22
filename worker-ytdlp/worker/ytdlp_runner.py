from __future__ import annotations

import json
import logging
import subprocess
from pathlib import Path
from typing import Any, Callable

from .manifest_builder import build_item, relative_staging_path

logger = logging.getLogger(__name__)


class YtdlpError(RuntimeError):
    pass


def probe_url(url: str, *, playlist_max_items: int | None = None, extra_args: list[str] | None = None) -> dict[str, Any]:
    cmd = [
        "yt-dlp",
        "--dump-single-json",
        "--flat-playlist",
        "--no-warnings",
        "--skip-download",
        *(extra_args or []),
    ]
    if playlist_max_items:
        cmd.extend(["--playlist-end", str(playlist_max_items)])
    cmd.append(url)
    result = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise YtdlpError(result.stderr.strip() or "yt-dlp probe failed")
    return json.loads(result.stdout)


def download_video(video_id: str, output_dir: Path) -> tuple[Path | None, dict[str, Any] | None]:
    output_dir.mkdir(parents=True, exist_ok=True)
    url = f"https://www.youtube.com/watch?v={video_id}"
    template = str(output_dir / f"{video_id}.%(ext)s")
    result = subprocess.run(
        [
            "yt-dlp",
            "-f",
            "bv*+ba/b",
            "--merge-output-format",
            "mp4",
            "-o",
            template,
            "--write-info-json",
            "--no-overwrites",
            "--no-warnings",
            url,
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    info_path = output_dir / f"{video_id}.info.json"
    info = None
    if info_path.exists():
        info = json.loads(info_path.read_text(encoding="utf-8"))

    video_files = [
        path
        for path in output_dir.glob(f"{video_id}.*")
        if path.suffix.lower() not in {".json", ".part", ".ytdl"}
    ]
    if result.returncode != 0 and not video_files:
        logger.warning("download failed for %s: %s", video_id, result.stderr.strip())
        return None, info
    if not video_files:
        return None, info
    return video_files[0], info


def download_items(
    *,
    video_ids: list[str],
    files_dir: Path,
    on_progress: Callable[[int, str], None] | None = None,
) -> list[dict[str, Any]]:
    items: list[dict[str, Any]] = []
    for index, video_id in enumerate(video_ids):
        if on_progress:
            on_progress(index, video_id)
        file_path, info = download_video(video_id, files_dir)
        if file_path is None:
            items.append(
                build_item(
                    video_id=video_id,
                    relative_path=f"files/{video_id}.mp4",
                    status="failed",
                    info=info,
                )
            )
            continue
        items.append(
            build_item(
                video_id=video_id,
                relative_path=relative_staging_path(files_dir, file_path),
                status="complete",
                info=info,
            )
        )
    return items
