from __future__ import annotations

from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from worker.__main__ import Worker
from worker.config import Config
from worker.track_download import (
    TrackDownloadError,
    download_audio_asset,
    download_subtitle_asset,
    download_track_asset,
)


def test_download_subtitle_asset_writes_file(tmp_path: Path):
    asset = {
        "id": "asset-1",
        "kind": "subtitle",
        "metadata": {
            "source_url": "https://example.com/en.vtt",
            "ext": "vtt",
        },
    }
    response = MagicMock()
    response.content = b"WEBVTT\n"
    response.raise_for_status = MagicMock()

    with patch("worker.track_download.requests.get", return_value=response) as get:
        out = download_subtitle_asset(asset, tmp_path)

    get.assert_called_once_with(
        "https://example.com/en.vtt",
        headers=None,
        timeout=120,
    )
    assert out == tmp_path / "asset-1.vtt"
    assert out.read_bytes() == b"WEBVTT\n"


def test_download_subtitle_asset_requires_source_url():
    with pytest.raises(TrackDownloadError, match="source_url"):
        download_subtitle_asset({"id": "a", "kind": "subtitle", "metadata": {}}, Path("."))


def test_download_audio_asset_builds_ytdlp_command(tmp_path: Path):
    asset = {
        "id": "asset-2",
        "kind": "audio",
        "metadata": {
            "format_id": "140",
            "source_page_url": "https://youtube.com/watch?v=abc",
        },
    }
    out_file = tmp_path / "asset-2.m4a"
    out_file.write_bytes(b"audio")

    with patch("worker.track_download.subprocess.run") as run:
        run.return_value = MagicMock(returncode=0, stdout="", stderr="")
        out = download_audio_asset(asset, tmp_path)

    cmd = run.call_args.args[0]
    assert cmd[:4] == ["yt-dlp", "-f", "140", "--no-playlist"]
    assert cmd[-1] == "https://youtube.com/watch?v=abc"
    assert out == out_file


def test_download_track_asset_routes_by_kind(tmp_path: Path):
    subtitle = {
        "id": "s1",
        "kind": "subtitle",
        "metadata": {"source_url": "https://example.com/a.vtt", "ext": "vtt"},
    }
    with patch("worker.track_download.requests.get") as get:
        get.return_value = MagicMock(content=b"x", raise_for_status=MagicMock())
        assert download_track_asset(subtitle, tmp_path).name == "s1.vtt"


def test_process_asset_job_success(tmp_path: Path):
    config = Config(
        core_url="http://core",
        vault_name="vault",
        vault_path=str(tmp_path),
        job_poll_secs=1,
        job_lease_secs=30,
        ytdlp_auto_update=False,
        ytdlp_update_on_start=False,
        ytdlp_update_interval_secs=3600,
            ytdlp_playlist_max_items=0,
            ytdlp_cookies_path=None,
        )
    worker = Worker(config)
    worker.client = MagicMock()
    worker.client.get_entity.return_value = {
        "assets": [
            {
                "id": "asset-1",
                "kind": "subtitle",
                "metadata": {
                    "source_url": "https://example.com/en.vtt",
                    "ext": "vtt",
                },
            }
        ]
    }
    worker.client.commit_asset.return_value = {"status": "present"}

    with patch("worker.track_download.requests.get") as get:
        get.return_value = MagicMock(content=b"WEBVTT", raise_for_status=MagicMock())
        worker.process_asset_job(
            {
                "id": "job-1",
                "type": "yt-dlp-asset",
                "input": '{"entity_id":"ent-1","asset_id":"asset-1"}',
            }
        )

    worker.client.commit_asset.assert_called_once()
    args = worker.client.commit_asset.call_args.args
    assert args[0] == "ent-1"
    assert args[1] == "asset-1"
    assert args[2] == "job-1"
    assert args[3].startswith("files/")


def test_process_asset_job_missing_asset_records_failure(tmp_path: Path):
    config = Config(
        core_url="http://core",
        vault_name="vault",
        vault_path=str(tmp_path),
        job_poll_secs=1,
        job_lease_secs=30,
        ytdlp_auto_update=False,
        ytdlp_update_on_start=False,
        ytdlp_update_interval_secs=3600,
            ytdlp_playlist_max_items=0,
            ytdlp_cookies_path=None,
        )
    worker = Worker(config)
    worker.client = MagicMock()
    worker.client.get_entity.return_value = {"assets": []}
    worker.record_asset_failure = MagicMock()

    with pytest.raises(TrackDownloadError):
        worker.process_asset_job(
            {
                "id": "job-2",
                "type": "yt-dlp-asset",
                "input": '{"entity_id":"ent-1","asset_id":"missing"}',
            }
        )

    worker.record_asset_failure.assert_called_once()
