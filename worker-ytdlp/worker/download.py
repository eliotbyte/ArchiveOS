from __future__ import annotations

import json
import subprocess
from pathlib import Path
from typing import Any, Callable

from .progress import parse_download_percent
from .validation import expected_best_height


class DownloadError(RuntimeError):
    pass


def probe_video(url: str, *, extra_args: list[str] | None = None) -> dict[str, Any]:
    cmd = [
        "yt-dlp",
        "--dump-single-json",
        "--skip-download",
        "--no-warnings",
        *(extra_args or []),
        url,
    ]
    result = subprocess.run(cmd, capture_output=True, text=True, check=False)
    if result.returncode != 0:
        raise DownloadError(result.stderr.strip() or "yt-dlp video probe failed")
    return json.loads(result.stdout)


def download_video(
    url: str,
    video_id: str,
    output_dir: Path,
    *,
    format_selector: str | None = "bv*+ba/b",
    extra_args: list[str] | None = None,
    on_progress: Callable[[float], None] | None = None,
) -> tuple[Path | None, dict[str, Any] | None, str | None]:
    output_dir.mkdir(parents=True, exist_ok=True)
    try:
        info = probe_video(url, extra_args=extra_args)
    except DownloadError as err:
        return None, None, str(err)

    if format_selector is None:
        return None, info, None

    page_url = info.get("webpage_url") or url
    template = str(output_dir / f"{video_id}.%(ext)s")
    cmd = [
        "yt-dlp",
        "-f",
        format_selector,
        "--merge-output-format",
        "mp4",
        "-o",
        template,
        "--write-info-json",
        "--no-overwrites",
        "--no-warnings",
        "--newline",
        *(extra_args or []),
        page_url,
    ]

    stderr_lines: list[str] = []
    returncode = 0
    if on_progress:
        process = subprocess.Popen(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
        )
        assert process.stdout is not None
        for line in process.stdout:
            stderr_lines.append(line)
            percent = parse_download_percent(line)
            if percent is not None:
                on_progress(percent)
        returncode = process.wait()
    else:
        result = subprocess.run(cmd, capture_output=True, text=True, check=False)
        returncode = result.returncode
        stderr_lines = (result.stderr or "").splitlines(keepends=True)

    info_path = output_dir / f"{video_id}.info.json"
    if info_path.exists():
        info = json.loads(info_path.read_text(encoding="utf-8"))

    video_files = [
        path
        for path in output_dir.glob(f"{video_id}.*")
        if path.suffix.lower() not in {".json", ".part", ".ytdl"}
    ]
    if returncode != 0 and not video_files:
        stderr = "".join(stderr_lines).strip() or "download failed"
        return None, info, stderr
    if not video_files:
        return None, info, "download produced no file"

    info["expected_height"] = expected_best_height(info)
    return video_files[0], info, None
