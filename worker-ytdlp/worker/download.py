from __future__ import annotations

import json
import subprocess
from pathlib import Path
from typing import Any

from .validation import expected_best_height


class DownloadError(RuntimeError):
    pass


def probe_video(video_id: str) -> dict[str, Any]:
    url = f"https://www.youtube.com/watch?v={video_id}"
    result = subprocess.run(
        [
            "yt-dlp",
            "--dump-single-json",
            "--skip-download",
            "--no-warnings",
            url,
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise DownloadError(result.stderr.strip() or "yt-dlp video probe failed")
    return json.loads(result.stdout)


def download_video(video_id: str, output_dir: Path) -> tuple[Path | None, dict[str, Any] | None, str | None]:
    output_dir.mkdir(parents=True, exist_ok=True)
    try:
        info = probe_video(video_id)
    except DownloadError as err:
        return None, None, str(err)

    url = info.get("webpage_url") or f"https://www.youtube.com/watch?v={video_id}"
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
    if info_path.exists():
        info = json.loads(info_path.read_text(encoding="utf-8"))

    video_files = [
        path
        for path in output_dir.glob(f"{video_id}.*")
        if path.suffix.lower() not in {".json", ".part", ".ytdl"}
    ]
    if result.returncode != 0 and not video_files:
        return None, info, result.stderr.strip() or "download failed"
    if not video_files:
        return None, info, "download produced no file"

    info["expected_height"] = expected_best_height(info)
    return video_files[0], info, None
