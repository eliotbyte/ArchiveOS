from pathlib import Path
from unittest.mock import MagicMock, patch

from worker.manifest_builder import (
    build_item,
    build_thumbnail_item,
    thumbnail_relation,
    with_thumbnail_metadata,
)
from worker.thumbnails import (
    best_thumbnail_url,
    download_thumbnail,
    extension_from_url,
    thumbnail_external_id,
    thumbnail_http_headers,
    thumbnail_url_candidates,
)


def test_thumbnail_external_id():
    assert thumbnail_external_id("abc123") == "abc123:thumbnail"


def test_best_thumbnail_url_prefers_highest_resolution():
    info = {
        "id": "abc123",
        "thumbnails": [
            {"url": "https://example.com/low.jpg", "width": 120, "height": 90},
            {"url": "https://example.com/high.webp", "width": 1280, "height": 720},
        ],
    }
    candidates = thumbnail_url_candidates(info, "abc123")
    assert best_thumbnail_url(info) == "https://example.com/high.webp"
    assert candidates[0] == "https://example.com/high.webp"
    assert candidates[-1].endswith("/default.jpg")


def test_best_thumbnail_url_prefers_widescreen_over_letterboxed_sd():
    """Brush Your Teeth case: sddefault 640x480 has black bars; mq/hq 16:9 variants do not."""
    info = {
        "id": "5YYqK5Ww_6g",
        "thumbnails": [
            {
                "url": "https://i.ytimg.com/vi/5YYqK5Ww_6g/sddefault.jpg",
                "width": 640,
                "height": 480,
            },
            {
                "url": "https://i.ytimg.com/vi/5YYqK5Ww_6g/mqdefault.jpg",
                "width": 320,
                "height": 180,
            },
            {
                "url": "https://i.ytimg.com/vi/5YYqK5Ww_6g/hqdefault.jpg?sqp=widescreen",
                "width": 336,
                "height": 188,
            },
        ],
    }
    candidates = thumbnail_url_candidates(info, "5YYqK5Ww_6g")
    assert "mqdefault" in candidates[0] or "sqp=widescreen" in candidates[0]
    assert "sddefault" not in candidates[0]


def test_thumbnail_url_candidates_static_mq_before_hq():
    info = {"id": "vid1", "thumbnails": []}
    candidates = thumbnail_url_candidates(info, "vid1")
    maxres_idx = next(i for i, url in enumerate(candidates) if "maxresdefault" in url)
    mq_idx = next(i for i, url in enumerate(candidates) if "mqdefault" in url)
    hq_idx = next(i for i, url in enumerate(candidates) if "hqdefault" in url)
    sd_idx = next(i for i, url in enumerate(candidates) if "sddefault" in url)
    assert maxres_idx < mq_idx < hq_idx < sd_idx


def test_extension_from_url():
    assert extension_from_url("https://i.ytimg.com/vi_webp/abc/maxresdefault.webp") == ".webp"
    assert extension_from_url("https://i.ytimg.com/vi/abc/hqdefault.jpg") == ".jpg"
    assert extension_from_url("https://example.com/noext") == ".jpg"


@patch("worker.thumbnails.urllib.request.urlopen")
def test_download_thumbnail_falls_back_when_maxres_404(mock_urlopen, tmp_path: Path):
    ok_response = MagicMock()
    ok_response.read.return_value = b"\x89PNG\r\n"
    ok_response.__enter__.return_value = ok_response
    mock_urlopen.side_effect = [
        Exception("HTTP Error 404: Not Found"),
        ok_response,
    ]

    info = {
        "id": "vid1",
        "thumbnails": [{"url": "https://i.ytimg.com/vi_webp/vid1/maxresdefault.webp"}],
    }
    path, error = download_thumbnail("vid1", info, tmp_path)

    assert error is None
    assert path is not None
    assert path.name == "vid1.jpg"
    assert mock_urlopen.call_count == 2


@patch("worker.thumbnails.urllib.request.urlopen")
def test_download_thumbnail_writes_file(mock_urlopen, tmp_path: Path):
    mock_response = MagicMock()
    mock_response.read.return_value = b"\x89PNG\r\n"
    mock_response.__enter__.return_value = mock_response
    mock_urlopen.return_value = mock_response

    info = {"thumbnails": [{"url": "https://example.com/thumb.webp"}]}
    path, error = download_thumbnail("vid1", info, tmp_path)

    assert error is None
    assert path is not None
    assert path.name == "vid1.webp"
    assert path.read_bytes() == b"\x89PNG\r\n"


def test_build_thumbnail_item_shape():
    item = build_thumbnail_item(
        video_id="vid1",
        relative_path="files/vid1.webp",
        info={"title": "My Video"},
        source_thumbnail_url="https://example.com/thumb.webp",
    )
    assert item["source_ref"]["kind"] == "thumbnail"
    assert item["source_ref"]["external_id"] == "vid1:thumbnail"
    assert item["metadata_by_provenance"]["archiveos"]["visibility"] == "hidden"
    assert item["metadata_by_provenance"]["archiveos"]["asset_role"] == "thumbnail"


def test_build_item_splits_metadata_by_provenance():
    item = build_item(
        video_id="abc123",
        relative_path="files/abc123.mp4",
        status="complete",
        info={"title": "One", "duration": 10, "thumbnail_external_id": "abc123:thumbnail"},
        file_meta={"actual_height": 720, "validation_status": "ok"},
    )
    assert item["metadata_by_provenance"]["yt-dlp"]["title"] == "One"
    assert item["metadata_by_provenance"]["ffprobe"]["actual_height"] == 720
    assert item["metadata_by_provenance"]["archiveos"]["thumbnail_external_id"] == "abc123:thumbnail"


def test_thumbnail_relation_shape():
    relation = thumbnail_relation("vid1", "youtube")
    assert relation["relation"] == "thumbnail"
    assert relation["to_external_id"] == "vid1:thumbnail"


def test_with_thumbnail_metadata_adds_external_id():
    info = with_thumbnail_metadata({"title": "t"}, "vid1")
    assert info["thumbnail_external_id"] == "vid1:thumbnail"


def test_thumbnail_http_headers_merges_ytdlp_headers():
    headers = thumbnail_http_headers(
        {"http_headers": {"Cookie": "abc=1", "Referer": "https://youtube.com"}}
    )
    assert headers["Cookie"] == "abc=1"
    assert headers["Referer"] == "https://youtube.com"
    assert "User-Agent" in headers
