from __future__ import annotations

from pathlib import Path

from worker.staging import (
    manifest_staging_paths,
    sanitize_manifest_for_staging,
    staging_file_exists,
)


def test_sanitize_manifest_drops_missing_avatar_and_thumbnail(tmp_path: Path):
    staging = tmp_path / "job"
    files_dir = staging / "files"
    files_dir.mkdir(parents=True)
    (files_dir / "abc.mp4").write_bytes(b"video")

    manifest = {
        "items": [
            {
                "status": "complete",
                "path": "files/abc.mp4",
                "source_ref": {"external_id": "abc"},
            },
            {
                "status": "complete",
                "path": "files/missing.jpg",
                "source_ref": {"external_id": "missing:thumbnail"},
            },
        ],
        "channels": [
            {
                "external_id": "UC_test",
                "avatar": {"path": "files/UC_test.jpg", "source_url": "http://x"},
            }
        ],
    }

    cleaned = sanitize_manifest_for_staging(manifest, staging)
    assert cleaned["items"][0]["status"] == "complete"
    assert cleaned["items"][1]["status"] == "failed"
    assert "avatar" not in cleaned["channels"][0]
    assert staging_file_exists(staging, "files/abc.mp4")


def test_manifest_staging_paths_collects_items_and_avatars():
    manifest = {
        "items": [{"status": "complete", "path": "files/v.mp4"}],
        "channels": [{"avatar": {"path": "files/ch.jpg"}}],
    }
    assert manifest_staging_paths(manifest) == ["files/v.mp4", "files/ch.jpg"]
