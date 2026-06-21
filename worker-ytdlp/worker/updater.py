from __future__ import annotations

import logging
import subprocess
import sys
import time
from typing import Callable

logger = logging.getLogger(__name__)


def ytdlp_version() -> str:
    result = subprocess.run(
        ["yt-dlp", "--version"],
        capture_output=True,
        text=True,
        check=False,
    )
    return (result.stdout or result.stderr or "unknown").strip()


def update_ytdlp() -> None:
    logger.info("updating yt-dlp via pip")
    subprocess.run(
        [sys.executable, "-m", "pip", "install", "--upgrade", "yt-dlp"],
        check=False,
    )
    logger.info("yt-dlp version after update: %s", ytdlp_version())


class YtdlpUpdater:
    def __init__(self, *, enabled: bool, on_start: bool, interval_secs: int) -> None:
        self.enabled = enabled
        self.on_start = on_start
        self.interval_secs = interval_secs
        self._last_update = 0.0

    def maybe_update(self, *, idle: bool) -> None:
        if not self.enabled:
            return
        now = time.monotonic()
        if self.on_start and self._last_update == 0.0:
            update_ytdlp()
            self._last_update = now
            self.on_start = False
            return
        if not idle:
            return
        if now - self._last_update >= self.interval_secs:
            update_ytdlp()
            self._last_update = now


def heartbeat_loop(
    *,
    job_id: str,
    lease_secs: int,
    send_heartbeat: Callable[[], None],
    interval_secs: int = 60,
    stop: Callable[[], bool],
) -> None:
    while not stop():
        time.sleep(interval_secs)
        if stop():
            break
        try:
            send_heartbeat()
            logger.debug("heartbeat job %s lease=%ss", job_id, lease_secs)
        except Exception:
            logger.exception("heartbeat failed for job %s", job_id)
