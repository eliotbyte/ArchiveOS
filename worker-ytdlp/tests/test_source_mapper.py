from worker.manifest_builder import (
    build_discovered_item,
    build_item,
    build_manifest,
    metadata_from_info_ytdlp,
)
from worker.source_mapper import extractor_from_info, extractor_from_probe
from worker.track_catalog import build_subtitle_catalog, build_track_catalog


def test_extractor_from_youtube_info():
    info = {"extractor_key": "Youtube", "webpage_url": "https://youtube.com/watch?v=abc"}
    assert extractor_from_info(info) == "youtube"


def test_extractor_from_pornhub_info():
    info = {"extractor_key": "Pornhub", "webpage_url": "https://www.pornhub.com/view_video.php?viewkey=x"}
    assert extractor_from_info(info) == "pornhub"


def test_metadata_from_info_skips_null_description():
    meta = metadata_from_info_ytdlp({"title": "T", "description": None})
    assert "description" not in meta
    assert meta["title"] == "T"


def test_build_item_includes_subtitle_catalog():
    info = {
        "extractor_key": "Pornhub",
        "id": "ph1",
        "title": "Example",
        "subtitles": {
            "en": [{"ext": "srt", "url": "https://example.com/en.srt"}],
        },
        "formats": [
            {"format_id": "audio-en", "acodec": "mp4a", "vcodec": "none", "language": "en"},
        ],
    }
    item = build_item(
        video_id="ph1",
        relative_path="files/ph1.mp4",
        status="complete",
        info=info,
        source="pornhub",
    )
    assert item["source_ref"]["source"] == "pornhub"
    assert any(asset["kind"] == "subtitle" for asset in item["assets"])
    assert any(asset["kind"] == "audio" for asset in item["assets"])


def test_build_manifest_sets_source_identity():
    probe = {
        "extractor_key": "Youtube",
        "_type": "video",
        "id": "abc123",
        "title": "One",
    }
    manifest = build_manifest(
        vault_name="archiveos",
        input_url="https://youtube.com/watch?v=abc123",
        probe=probe,
        items=[
            build_item(
                video_id="abc123",
                relative_path="files/abc123.mp4",
                status="complete",
                info={"extractor_key": "Youtube", "title": "One"},
                source="youtube",
            )
        ],
        channels=[],
        relations=[],
        source="youtube",
    )
    assert manifest["source_identity"] == "youtube"
    assert manifest["source"] == "yt-dlp"


def test_build_manifest_playlist_collection_type_is_source_prefixed():
    probe = {
        "extractor_key": "Youtube",
        "_type": "playlist",
        "id": "PLtest",
        "webpage_url": "https://youtube.com/playlist?list=PLtest",
        "entries": [{"id": "v1"}],
    }
    manifest = build_manifest(
        vault_name="archiveos",
        input_url=probe["webpage_url"],
        probe=probe,
        items=[],
        channels=[],
        relations=[],
        source="youtube",
    )
    assert manifest["collection"]["type"] == "youtube_playlist"


def test_build_discovered_item_uses_source():
    item = build_discovered_item(
        {"id": "v1", "extractor_key": "Pornhub", "title": "One"},
        source="pornhub",
    )
    assert item["source_ref"]["source"] == "pornhub"


def test_subtitle_catalog_track_keys_are_stable():
    assets = build_subtitle_catalog(
        {
            "webpage_url": "https://youtube.com/watch?v=vid1",
            "subtitles": {
                "en": [{"ext": "vtt", "url": "https://example.com/en.vtt"}],
            }
        },
        "vid1",
    )
    assert assets[0]["track_key"] == "subtitle:en:vtt:manual"
    assert assets[0]["status"] == "remote"
    assert assets[0]["metadata"]["source_page_url"] == "https://youtube.com/watch?v=vid1"


def test_audio_catalog_includes_source_page_url_and_format_id():
    assets = build_track_catalog(
        {
            "webpage_url": "https://youtube.com/watch?v=vid1",
            "formats": [
                {
                    "format_id": "a1",
                    "acodec": "mp4a",
                    "vcodec": "none",
                    "language": "en",
                    "url": "https://example.com/audio.m4a",
                },
            ],
        },
        "vid1",
    )
    audio_assets = [asset for asset in assets if asset["kind"] == "audio"]
    assert len(audio_assets) == 1
    assert audio_assets[0]["metadata"]["source_page_url"] == "https://youtube.com/watch?v=vid1"
    assert audio_assets[0]["metadata"]["format_id"] == "a1"


def test_track_catalog_deduplicates_audio_formats():
    assets = build_track_catalog(
        {
            "formats": [
                {"format_id": "a1", "acodec": "mp4a", "vcodec": "none", "language": "en"},
                {"format_id": "a1", "acodec": "mp4a", "vcodec": "none", "language": "en"},
            ]
        },
        "vid1",
    )
    audio_assets = [asset for asset in assets if asset["kind"] == "audio"]
    assert len(audio_assets) == 1
