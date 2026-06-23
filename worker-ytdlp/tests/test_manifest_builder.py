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
        {"external_id": "v1", "position": 0, "kind": "video", "url": None},
        {"external_id": "v2", "position": 1, "kind": "video", "url": None},
    ]


def test_build_manifest_adds_discovered_items_for_undownloaded_playlist_entries():
    probe = {
        "_type": "playlist",
        "id": "PLtest",
        "title": "My Playlist",
        "webpage_url": "https://youtube.com/playlist?list=PLtest",
        "entries": [
            {"id": "v1", "url": "https://youtube.com/watch?v=v1", "title": "One"},
            {"id": "v2", "url": "https://youtube.com/watch?v=v2", "title": "Two"},
        ],
    }
    manifest = build_manifest(
        vault_name="archiveos",
        input_url=probe["webpage_url"],
        probe=probe,
        items=[
            build_item(
                video_id="v2",
                relative_path="files/v2.mp4",
                status="complete",
                info={"id": "v2", "title": "Two"},
            )
        ],
        channels=[],
        relations=[],
    )

    discovered = [
        item for item in manifest["items"]
        if item["source_ref"]["external_id"] == "v1"
    ]
    assert discovered[0]["status"] == "discovered"


def test_build_manifest_channel_probe_uses_channel_uploads_collection():
    probe = {
        "_type": "playlist",
        "id": "UC123",
        "channel_id": "UC123",
        "channel": "Test Channel",
        "webpage_url": "https://youtube.com/channel/UC123/videos",
        "entries": [{"id": "v1"}],
    }
    manifest = build_manifest(
        vault_name="archiveos",
        input_url=probe["webpage_url"],
        probe=probe,
        items=[],
        channels=[],
        relations=[],
    )

    assert manifest["collection"]["type"] == "youtube_channel_uploads"


def test_build_manifest_channel_probe_adds_channel_entity():
    probe = {
        "_type": "playlist",
        "id": "UC123",
        "channel_id": "UC123",
        "channel": "Test Channel",
        "channel_url": "https://youtube.com/channel/UC123",
        "webpage_url": "https://youtube.com/channel/UC123/videos",
        "entries": [{"id": "v1"}],
    }
    manifest = build_manifest(
        vault_name="archiveos",
        input_url=probe["webpage_url"],
        probe=probe,
        items=[],
        channels=[],
        relations=[],
    )

    assert manifest["channels"][0]["external_id"] == "UC123"


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


def test_classify_deleted_video():
    kind, retryable = classify_error("ERROR: Video has been removed by the uploader.")
    assert kind == "dead"
    assert retryable is False


def test_error_kind_to_item_status_maps_lifecycle():
    from worker.failures import error_kind_to_item_status

    assert error_kind_to_item_status("private") == "private"
    assert error_kind_to_item_status("dead") == "dead"
    assert error_kind_to_item_status("unavailable") == "unavailable"
    assert error_kind_to_item_status("region_locked") == "region_locked"
    assert error_kind_to_item_status("unknown") == "failed"


def test_build_manifest_preserves_enriched_channel_avatar():
    probe = {
        "_type": "video",
        "id": "abc123",
        "title": "One",
        "channel_id": "UC123",
        "channel": "Test Channel",
        "channel_url": "https://youtube.com/channel/UC123",
    }
    enriched_channel = {
        "source": "youtube",
        "kind": "channel",
        "external_id": "UC123",
        "url": "https://youtube.com/channel/UC123",
        "metadata": {"title": "Test Channel"},
        "avatar": {
            "path": "files/UC123.jpg",
            "source_url": "https://example.com/avatar.jpg",
        },
    }
    manifest = build_manifest(
        vault_name="archiveos",
        input_url="https://youtube.com/watch?v=abc123",
        probe=probe,
        items=[],
        channels=[enriched_channel],
        relations=[],
    )
    assert len(manifest["channels"]) == 1
    channel = manifest["channels"][0]
    assert channel["avatar"]["path"] == "files/UC123.jpg"
    assert channel["metadata"]["title"] == "Test Channel"


def test_channel_from_info_and_relation():
    channel = channel_from_info(
        {
            "extractor_key": "Youtube",
            "channel_id": "UC123",
            "channel": "Test Channel",
            "channel_url": "https://youtube.com/channel/UC123",
            "channel_follower_count": 42,
            "channel_is_verified": True,
        }
    )
    assert channel is not None
    assert channel["external_id"] == "UC123"
    relation = uploaded_by_relation("vid1", "UC123", "youtube")
    assert relation["relation"] == "uploaded_by"
