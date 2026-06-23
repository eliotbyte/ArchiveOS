from __future__ import annotations

import re
import threading
import time
from typing import Any, Callable

_DOWNLOAD_PERCENT_RE = re.compile(r"\[download\]\s+([\d.]+)%")


def parse_download_percent(line: str) -> float | None:
    match = _DOWNLOAD_PERCENT_RE.search(line)
    if not match:
        return None
    try:
        return float(match.group(1))
    except ValueError:
        return None


class ProgressReporter:
    def __init__(self, send_heartbeat: Callable[[dict[str, Any] | None], None]) -> None:
        self._send = send_heartbeat
        self._state: dict[str, Any] | None = None
        self._last_sent = 0.0
        self._lock = threading.Lock()

    def update(self, state: dict[str, Any], *, force: bool = False) -> None:
        with self._lock:
            self._state = state
            now = time.monotonic()
            if not force and now - self._last_sent < 1.5:
                return
            self._last_sent = now
            payload = state
        self._send(payload)

    def tick(self) -> None:
        with self._lock:
            payload = self._state
        self._send(payload)


def video_labels_from_probe(probe: dict[str, Any]) -> dict[str, str]:
    labels: dict[str, str] = {}
    entries = probe.get("entries") or []
    if probe.get("id") and not entries:
        entries = [probe]
    for entry in entries:
        if not entry or not entry.get("id"):
            continue
        video_id = entry["id"]
        title = entry.get("title") or video_id
        labels[video_id] = str(title)
    return labels


def build_progress(
    *,
    phase: str,
    current: int | None = None,
    total: int | None = None,
    label: str | None = None,
    percent: float | None = None,
    steps: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    payload: dict[str, Any] = {"phase": phase}
    if current is not None:
        payload["current"] = current
    if total is not None:
        payload["total"] = total
    if label:
        payload["label"] = label
    if percent is not None:
        payload["percent"] = percent
    if steps:
        payload["steps"] = steps
    return payload


def build_video_steps(
    video_ids: list[str],
    labels: dict[str, str],
    *,
    running_id: str | None = None,
    running_percent: float | None = None,
    done_ids: set[str] | None = None,
    failed_ids: set[str] | None = None,
) -> list[dict[str, Any]]:
    done_ids = done_ids or set()
    failed_ids = failed_ids or set()
    steps: list[dict[str, Any]] = []
    for video_id in video_ids:
        if video_id in failed_ids:
            status = "failed"
            percent = None
        elif video_id in done_ids:
            status = "done"
            percent = None
        elif video_id == running_id:
            status = "running"
            percent = running_percent
        else:
            status = "pending"
            percent = None
        steps.append(
            {
                "id": video_id,
                "label": labels.get(video_id, video_id),
                "status": status,
                **({"percent": percent} if percent is not None else {}),
            }
        )
    return steps
