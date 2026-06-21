from __future__ import annotations

from typing import Any


def channel_from_info(info: dict[str, Any]) -> dict[str, Any] | None:
    channel_id = info.get("channel_id")
    if not channel_id:
        return None

    metadata: dict[str, Any] = {}
    if title := info.get("channel") or info.get("uploader"):
        metadata["title"] = title
    if description := info.get("channel_description") or info.get("description"):
        metadata["description"] = description
    if followers := info.get("channel_follower_count"):
        metadata["follower_count"] = followers
    if info.get("channel_is_verified") is not None:
        metadata["verified"] = info.get("channel_is_verified")
    if uploader_id := info.get("uploader_id"):
        metadata["uploader_id"] = uploader_id
    if uploader_url := info.get("uploader_url"):
        metadata["uploader_url"] = uploader_url
    thumbnails = info.get("thumbnails") or []
    if thumbnails:
        metadata["thumbnail_url"] = thumbnails[-1].get("url")

    return {
        "source": "youtube",
        "kind": "channel",
        "external_id": channel_id,
        "url": info.get("channel_url"),
        "metadata": metadata or None,
    }


def uploaded_by_relation(video_id: str, channel_id: str) -> dict[str, str]:
    return {
        "source": "youtube",
        "from_kind": "video",
        "from_external_id": video_id,
        "to_kind": "channel",
        "to_external_id": channel_id,
        "relation": "uploaded_by",
    }
