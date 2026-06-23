from worker.job_status import (
    LIFECYCLE_STATUSES,
    resolve_status_from_manifest,
    should_download,
)


def test_should_download_skips_unavailable_without_resync():
    state = {"source_status": "unavailable", "has_present_asset": False}
    assert should_download(state, resync=False) is False


def test_should_download_retries_unavailable_on_resync():
    state = {"source_status": "unavailable", "has_present_asset": False}
    assert should_download(state, resync=True) is True


def test_should_download_never_retries_dead():
    state = {"source_status": "dead", "has_present_asset": False}
    assert should_download(state, resync=True) is False


def test_should_download_skips_present_asset_even_on_resync():
    state = {"source_status": "unavailable", "has_present_asset": True}
    assert should_download(state, resync=True) is False


def test_resolve_status_one_unavailable_many_discovered_is_done():
    manifest = {
        "items": [
            {
                "status": "discovered",
                "source_ref": {"kind": "video", "external_id": f"v{i}"},
            }
            for i in range(15)
        ]
        + [
            {
                "status": "unavailable",
                "source_ref": {"kind": "video", "external_id": "missing"},
            }
        ]
    }
    assert resolve_status_from_manifest(manifest) == "partial"


def test_resolve_status_hard_fail_without_complete_is_failed():
    manifest = {
        "items": [
            {
                "status": "failed",
                "source_ref": {"kind": "video", "external_id": "bad"},
            }
        ]
    }
    assert resolve_status_from_manifest(manifest) == "failed"


def test_lifecycle_statuses_include_expected_values():
    assert "unavailable" in LIFECYCLE_STATUSES
    assert "dead" in LIFECYCLE_STATUSES
