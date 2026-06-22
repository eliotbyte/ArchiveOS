from __future__ import annotations

from pathlib import Path

WORKERS_DIR = "workers"
YTDLP_DIR = "ytdlp"
THUMBNAIL_DIR = "thumbnail"
COOKIES_DIR = "cookies"
CACHE_DIR = "cache"
DEFAULT_YOUTUBE_COOKIES = "youtube.txt"


def ytdlp_worker_dir(vault_path: str | Path) -> Path:
    return Path(vault_path) / WORKERS_DIR / YTDLP_DIR


def ytdlp_cookies_dir(vault_path: str | Path) -> Path:
    return ytdlp_worker_dir(vault_path) / COOKIES_DIR


def ytdlp_cache_dir(vault_path: str | Path) -> Path:
    return ytdlp_worker_dir(vault_path) / CACHE_DIR


def thumbnail_worker_dir(vault_path: str | Path) -> Path:
    return Path(vault_path) / WORKERS_DIR / THUMBNAIL_DIR


def thumbnail_cache_dir(vault_path: str | Path) -> Path:
    return thumbnail_worker_dir(vault_path) / CACHE_DIR


def default_youtube_cookies_path(vault_path: str | Path) -> Path:
    return ytdlp_cookies_dir(vault_path) / DEFAULT_YOUTUBE_COOKIES


def ensure_worker_dirs(vault_path: str | Path) -> None:
    ytdlp_cookies_dir(vault_path).mkdir(parents=True, exist_ok=True)
    ytdlp_cache_dir(vault_path).mkdir(parents=True, exist_ok=True)
    thumbnail_cache_dir(vault_path).mkdir(parents=True, exist_ok=True)
