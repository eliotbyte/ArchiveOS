from __future__ import annotations

from .config import Config


def cookies_args(config: Config) -> list[str]:
    if config.ytdlp_cookies_path:
        return ["--cookies", config.ytdlp_cookies_path]
    return []


def ytdlp_extra_args(config: Config) -> list[str]:
    """Cookies + YouTube JS challenge solver (requires deno in image)."""
    return ["--remote-components", "ejs:github", *cookies_args(config)]
