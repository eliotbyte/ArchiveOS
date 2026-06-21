from __future__ import annotations

import json
import subprocess
from pathlib import Path
from typing import Any


class ValidationError(RuntimeError):
    pass


def ffprobe_file(path: Path) -> dict[str, Any]:
    result = subprocess.run(
        [
            "ffprobe",
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            str(path),
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise ValidationError(result.stderr.strip() or "ffprobe failed")
    return json.loads(result.stdout)


def best_video_stream(probe: dict[str, Any]) -> dict[str, Any] | None:
    streams = [s for s in probe.get("streams", []) if s.get("codec_type") == "video"]
    if not streams:
        return None
    return max(streams, key=lambda s: int(s.get("height") or 0))


def best_audio_stream(probe: dict[str, Any]) -> dict[str, Any] | None:
    streams = [s for s in probe.get("streams", []) if s.get("codec_type") == "audio"]
    return streams[0] if streams else None


def expected_best_height(info: dict[str, Any]) -> int | None:
    formats = info.get("formats") or []
    heights = [int(fmt["height"]) for fmt in formats if fmt.get("height")]
    if heights:
        return max(heights)
    if info.get("height"):
        return int(info["height"])
    return None


def validate_download(
    *,
    file_path: Path,
    info: dict[str, Any] | None,
) -> tuple[dict[str, Any], dict[str, Any]]:
    probe = ffprobe_file(file_path)
    video = best_video_stream(probe)
    if video is None:
        raise ValidationError("downloaded file has no video stream")

    expected_height = expected_best_height(info or {})
    actual_height = int(video.get("height") or 0)
    if expected_height and actual_height and actual_height < expected_height:
        raise ValidationError(
            f"quality mismatch: expected up to {expected_height}p, got {actual_height}p"
        )

    audio = best_audio_stream(probe)
    if audio is None and not (info or {}).get("acodec") == "none":
        raise ValidationError("downloaded file has no audio stream")

    file_meta = {
        "container_format": probe.get("format", {}).get("format_name"),
        "duration": probe.get("format", {}).get("duration"),
        "bitrate": probe.get("format", {}).get("bit_rate"),
        "video_codec": video.get("codec_name"),
        "width": video.get("width"),
        "height": video.get("height"),
        "fps": video.get("avg_frame_rate") or video.get("r_frame_rate"),
        "pixel_format": video.get("pix_fmt"),
        "audio_codec": audio.get("codec_name") if audio else None,
        "audio_channels": audio.get("channels") if audio else None,
        "audio_sample_rate": audio.get("sample_rate") if audio else None,
        "expected_height": expected_height,
        "actual_height": actual_height,
        "validation_status": "ok",
    }
    return probe, file_meta
