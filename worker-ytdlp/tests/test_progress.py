from worker.progress import (
    build_progress,
    build_video_steps,
    parse_download_percent,
    video_labels_from_probe,
)


def test_parse_download_percent() -> None:
    assert parse_download_percent("[download]  45.2% of ~  123.45MiB at  1.23MiB/s ETA 00:30") == 45.2
    assert parse_download_percent("some other line") is None


def test_build_video_steps_running() -> None:
    steps = build_video_steps(
        ["a", "b", "c"],
        {"a": "Alpha", "b": "Beta", "c": "Gamma"},
        running_id="b",
        running_percent=33.0,
        done_ids={"a"},
    )
    assert steps[0]["status"] == "done"
    assert steps[1]["status"] == "running"
    assert steps[1]["percent"] == 33.0
    assert steps[2]["status"] == "pending"


def test_video_labels_from_probe_playlist() -> None:
    probe = {
        "entries": [
            {"id": "v1", "title": "First"},
            {"id": "v2", "title": "Second"},
        ]
    }
    assert video_labels_from_probe(probe) == {"v1": "First", "v2": "Second"}


def test_build_progress_minimal() -> None:
    payload = build_progress(phase="discovering", label="https://example.com")
    assert payload == {"phase": "discovering", "label": "https://example.com"}
