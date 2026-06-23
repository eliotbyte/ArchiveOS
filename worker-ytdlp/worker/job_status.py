from __future__ import annotations

from typing import Any

LIFECYCLE_STATUSES = frozenset({"unavailable", "private", "region_locked", "dead"})


def should_download(state: dict[str, Any] | None, *, resync: bool = False) -> bool:
    if not state:
        return True
    if state.get("entity_status") == "user_deleted":
        return False
    if state.get("has_present_asset"):
        return False
    source_status = state.get("source_status")
    if source_status == "dead":
        return False
    if resync and source_status in {"unavailable", "region_locked", "private"}:
        return True
    if source_status in LIFECYCLE_STATUSES:
        return False
    return True


def resolve_status_from_manifest(manifest: dict[str, Any]) -> str:
    video_items = [
        item
        for item in manifest.get("items", [])
        if (item.get("source_ref") or {}).get("kind") == "video"
    ]
    if not video_items:
        return "done"

    complete = sum(1 for item in video_items if item.get("status") == "complete")
    discovered = sum(1 for item in video_items if item.get("status") == "discovered")
    lifecycle = sum(
        1 for item in video_items if item.get("status") in LIFECYCLE_STATUSES
    )
    hard_failed = sum(
        1
        for item in video_items
        if item.get("status")
        not in {"complete", "discovered", *LIFECYCLE_STATUSES}
    )

    if hard_failed > 0 and complete > 0:
        return "partial"
    if hard_failed > 0:
        return "failed"
    if lifecycle > 0:
        return "partial"
    return "done"
