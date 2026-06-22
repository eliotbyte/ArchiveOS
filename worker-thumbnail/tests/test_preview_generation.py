from __future__ import annotations

from pathlib import Path

import pytest

from worker.__main__ import (
    ROLE_LARGE,
    generate_image_previews,
    generate_video_previews,
    is_supported_mime,
    managed_blob_path,
)


def test_is_supported_mime() -> None:
    assert is_supported_mime("image/jpeg")
    assert is_supported_mime("video/mp4")
    assert not is_supported_mime("application/pdf")


def test_managed_blob_path_resolves_content_hash(tmp_path: Path) -> None:
    content_hash = "a" * 64
    shard = tmp_path / "blobs" / content_hash[:2] / content_hash[2:4]
    shard.mkdir(parents=True)
    blob = shard / f"{content_hash}.mp4"
    blob.write_bytes(b"video")

    asset = {
        "content_hash": content_hash,
        "mime": "video/mp4",
        "ext": ".mp4",
        "path": None,
    }
    resolved = managed_blob_path(asset, tmp_path)
    assert resolved == blob


def test_generate_image_previews(tmp_path: Path) -> None:
    source = tmp_path / "source.png"
    from PIL import Image

    Image.new("RGB", (640, 360), color=(255, 0, 0)).save(source)
    staging = tmp_path / "staging"
    files = generate_image_previews(source, staging)
    roles = {item["preview_role"] for item in files}
    assert "preview_image_small" in roles
    assert "preview_image_large" in roles


def test_generate_video_previews_includes_large(tmp_path: Path, monkeypatch) -> None:
    source = tmp_path / "source.mp4"
    source.write_bytes(b"not-a-real-video")

    def fake_probe(_: Path) -> float:
        return 12.0

    def fake_extract(source_path: Path, target: Path, timestamp: float) -> None:
        from PIL import Image

        target.parent.mkdir(parents=True, exist_ok=True)
        Image.new("RGB", (160, 90), color=(0, 0, 255)).save(target)

    monkeypatch.setattr("worker.__main__.probe_duration", fake_probe)
    monkeypatch.setattr("worker.__main__.extract_frame", fake_extract)

    def fake_write_webp(source_path: Path, target: Path, size: tuple[int, int]) -> None:
        from PIL import Image

        target.parent.mkdir(parents=True, exist_ok=True)
        Image.new("RGB", size, color=(0, 128, 0)).save(target, format="WEBP")

    monkeypatch.setattr("worker.__main__.write_webp", fake_write_webp)

    staging = tmp_path / "staging"
    files = generate_video_previews(source, staging)
    roles = {item["preview_role"] for item in files}
    assert ROLE_LARGE in roles
    assert "timeline_sprite" in roles
    assert "timeline_manifest" in roles
