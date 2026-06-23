from __future__ import annotations

from typing import Any

from .source_mapper import extractor_from_info


def channel_from_info(info: dict[str, Any]) -> dict[str, Any] | None:
    source = extractor_from_info(info)
    channel_id = info.get("channel_id")
    uploader_id = info.get("uploader_id")
    external_id = channel_id or uploader_id
    if not external_id:
        return None

    metadata: dict[str, Any] = {}
    if title := info.get("channel") or info.get("uploader"):
        metadata["title"] = title
    description = info.get("channel_description") or info.get("description")
    if description:
        metadata["description"] = description
    if followers := info.get("channel_follower_count"):
        metadata["follower_count"] = followers
    if info.get("channel_is_verified") is not None:
        metadata["verified"] = info.get("channel_is_verified")
    if uploader_id:
        metadata["uploader_id"] = uploader_id
    if uploader_url := info.get("uploader_url"):
        metadata["uploader_url"] = uploader_url
    if channel_url := info.get("channel_url"):
        metadata["channel_url"] = channel_url

    return {
        "source": source,
        "kind": "channel",
        "external_id": external_id,
        "url": info.get("channel_url") or info.get("uploader_url"),
        "metadata": metadata or None,
    }


def uploaded_by_relation(video_id: str, channel_id: str, source: str) -> dict[str, str]:
    return {
        "source": source,
        "from_kind": "video",
        "from_external_id": video_id,
        "to_kind": "channel",
        "to_external_id": channel_id,
        "relation": "uploaded_by",
    }
