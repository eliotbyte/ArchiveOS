from worker.channels import channel_from_info, uploaded_by_relation
from worker.discovery import is_playlist, list_video_ids
from worker.failures import classify_error
from worker.manifest_builder import (
    build_collection,
    build_item,
    build_manifest,
    build_membership,
    metadata_from_info,
)
from worker.validation import expected_best_height


def test_list_video_ids_playlist():
    probe = {
        "_type": "playlist",
        "id": "PLtest",
        "entries": [{"id": "v1"}, {"id": "v2"}],
    }
    assert list_video_ids(probe) == ["v1", "v2"]


def test_build_manifest_single_video():
    probe = {"_type": "video", "id": "abc123", "title": "One"}
    items = [
        build_item(
            video_id="abc123",
            relative_path="files/abc123.mp4",
            status="complete",
            info={"title": "One", "duration": 10, "height": 720},
            file_meta={"actual_height": 720, "validation_status": "ok"},
        )
    ]
    manifest = build_manifest(
        vault_name="archiveos",
        input_url="https://youtube.com/watch?v=abc123",
        probe=probe,
        items=items,
        channels=[],
        relations=[],
    )
    assert manifest["vault"] == "archiveos"
    assert "collection" not in manifest
    assert manifest["items"][0]["metadata_by_provenance"]["ffprobe"]["validation_status"] == "ok"


def test_build_manifest_playlist_with_membership():
    probe = {
        "_type": "playlist",
        "id": "PLtest",
        "title": "My Playlist",
        "webpage_url": "https://youtube.com/playlist?list=PLtest",
        "entries": [{"id": "v1"}, {"id": "v2"}],
    }
    items = [
        build_item(
            video_id="v2",
            relative_path="files/v2.mp4",
            status="complete",
            info={"title": "Second"},
        )
    ]
    manifest = build_manifest(
        vault_name="archiveos",
        input_url=probe["webpage_url"],
        probe=probe,
        items=items,
        channels=[],
        relations=[],
    )
    assert manifest["collection"]["external_id"] == "PLtest"
    assert manifest["membership"] == [
        {"external_id": "v1", "position": 0},
        {"external_id": "v2", "position": 1},
    ]


def test_metadata_from_info_formats_upload_date():
    meta = metadata_from_info(
        {
            "title": "t",
            "channel": "c",
            "upload_date": "20210101",
            "duration": 99,
        }
    )
    assert meta["upload_date"] == "2021-01-01"
    assert meta["duration"] == 99


def test_expected_best_height_from_formats():
    info = {
        "formats": [
            {"height": 360},
            {"height": 1080},
            {"height": 720},
        ]
    }
    assert expected_best_height(info) == 1080


def test_classify_private_error():
    kind, retryable = classify_error("ERROR: Private video. Sign in if you've been granted access.")
    assert kind == "private"
    assert retryable is False


def test_channel_from_info_and_relation():
    channel = channel_from_info(
        {
            "channel_id": "UC123",
            "channel": "Test Channel",
            "channel_url": "https://youtube.com/channel/UC123",
            "channel_follower_count": 42,
            "channel_is_verified": True,
        }
    )
    assert channel is not None
    assert channel["external_id"] == "UC123"
    relation = uploaded_by_relation("vid1", "UC123")
    assert relation["relation"] == "uploaded_by"
