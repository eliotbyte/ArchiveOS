from __future__ import annotations

from worker.asset_policy import AssetPolicy, should_download_channel_avatar
from worker.channel_avatars import (
    best_channel_avatar_url,
    resolve_author_probe_url,
)
from worker.channels import channel_from_info


def test_channel_from_info_omits_video_thumbnail_url():
    info = {
        "extractor": "youtube",
        "channel_id": "UC123",
        "channel": "Test Channel",
        "thumbnails": [{"url": "https://example.com/video-thumb.jpg"}],
    }
    channel = channel_from_info(info)
    assert channel is not None
    metadata = channel["metadata"] or {}
    assert "thumbnail_url" not in metadata


def test_resolve_author_probe_url_prefers_channel_url():
    info = {"channel_url": "https://youtube.com/channel/UC1", "uploader_url": "https://youtube.com/@u"}
    assert resolve_author_probe_url(info) == "https://youtube.com/channel/UC1"


def test_best_channel_avatar_url_prefers_avatar_uncropped():
    probe = {
        "thumbnails": [
            {"id": "banner", "url": "https://example.com/banner.jpg"},
            {"id": "avatar_uncropped", "url": "https://example.com/avatar.jpg"},
        ]
    }
    assert best_channel_avatar_url(probe) == "https://example.com/avatar.jpg"


def test_should_download_channel_avatar_defaults_true():
    assert should_download_channel_avatar(AssetPolicy()) is True
    assert should_download_channel_avatar(AssetPolicy(channel_avatar=False)) is False
