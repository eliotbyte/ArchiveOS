from __future__ import annotations

from .config import Config


def cookies_args(config: Config) -> list[str]:
    if config.ytdlp_cookies_path:
        return ["--cookies", config.ytdlp_cookies_path]
    return []
