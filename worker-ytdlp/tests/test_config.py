from __future__ import annotations

import os
from pathlib import Path

from worker.config import Config
from worker.paths import default_youtube_cookies_path, ytdlp_cache_dir, ytdlp_cookies_dir


def test_config_creates_worker_dirs(tmp_path: Path, monkeypatch):
    monkeypatch.setenv("VAULT_PATH", str(tmp_path))
    monkeypatch.setenv("VAULT_NAME", "vault")
    monkeypatch.delenv("YTDLP_COOKIES_PATH", raising=False)

    config = Config.from_env()

    assert Path(config.ytdlp_worker_dir).is_dir()
    assert Path(config.ytdlp_cache_dir) == ytdlp_cache_dir(tmp_path)
    assert Path(config.ytdlp_cookies_dir) == ytdlp_cookies_dir(tmp_path)
    assert config.ytdlp_cookies_path is None


def test_config_uses_default_youtube_cookies_when_present(tmp_path: Path, monkeypatch):
    monkeypatch.setenv("VAULT_PATH", str(tmp_path))
    monkeypatch.setenv("VAULT_NAME", "vault")
    monkeypatch.delenv("YTDLP_COOKIES_PATH", raising=False)

    cookies = default_youtube_cookies_path(tmp_path)
    cookies.parent.mkdir(parents=True, exist_ok=True)
    cookies.write_text("# Netscape HTTP Cookie File\n", encoding="utf-8")

    config = Config.from_env()
    assert config.ytdlp_cookies_path == str(cookies)


def test_config_explicit_cookies_override(tmp_path: Path, monkeypatch):
    monkeypatch.setenv("VAULT_PATH", str(tmp_path))
    monkeypatch.setenv("VAULT_NAME", "vault")
    custom = tmp_path / "custom.txt"
    custom.write_text("cookies", encoding="utf-8")
    monkeypatch.setenv("YTDLP_COOKIES_PATH", str(custom))

    config = Config.from_env()
    assert config.ytdlp_cookies_path == str(custom)
