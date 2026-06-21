import json
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from worker.failures import classify_error
from worker.validation import ValidationError, expected_best_height, validate_download


def test_classify_validation_error():
    kind, retryable = classify_error("quality mismatch: expected up to 1080p, got 360p")
    assert kind == "unknown"
    assert retryable is True


def test_classify_network_error():
    kind, retryable = classify_error("Unable to download webpage: HTTP Error 503")
    assert kind == "network"
    assert retryable is True


def test_expected_best_height_prefers_formats():
    assert expected_best_height({"height": 480, "formats": [{"height": 720}]}) == 720


@patch("worker.validation.subprocess.run")
def test_validate_download_ok(mock_run, tmp_path: Path):
    video_file = tmp_path / "clip.mp4"
    video_file.write_bytes(b"fake")
    probe_payload = {
        "format": {"format_name": "mp4", "duration": "10.0", "bit_rate": "1000"},
        "streams": [
            {
                "codec_type": "video",
                "codec_name": "h264",
                "width": 1280,
                "height": 720,
                "avg_frame_rate": "30/1",
                "pix_fmt": "yuv420p",
            },
            {
                "codec_type": "audio",
                "codec_name": "aac",
                "channels": 2,
                "sample_rate": "48000",
            },
        ],
    }
    mock_run.return_value = MagicMock(returncode=0, stdout=json.dumps(probe_payload), stderr="")

    _, file_meta = validate_download(
        file_path=video_file,
        info={"formats": [{"height": 720}]},
    )
    assert file_meta["validation_status"] == "ok"
    assert file_meta["actual_height"] == 720


@patch("worker.validation.subprocess.run")
def test_validate_download_height_mismatch(mock_run, tmp_path: Path):
    video_file = tmp_path / "clip.mp4"
    video_file.write_bytes(b"fake")
    probe_payload = {
        "format": {"format_name": "mp4"},
        "streams": [
            {"codec_type": "video", "codec_name": "h264", "width": 640, "height": 360},
            {"codec_type": "audio", "codec_name": "aac", "channels": 2},
        ],
    }
    mock_run.return_value = MagicMock(returncode=0, stdout=json.dumps(probe_payload), stderr="")

    with pytest.raises(ValidationError, match="quality mismatch"):
        validate_download(
            file_path=video_file,
            info={"formats": [{"height": 1080}]},
        )
