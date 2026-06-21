from __future__ import annotations

from pathlib import Path
from typing import Any

from .thumbnails import thumbnail_external_id


def list_entries(probe: dict[str, Any]) -> list[dict[str, Any]]:
    if probe.get("_type") == "playlist" or probe.get("entries"):
        entries = probe.get("entries") or []
        return [entry for entry in entries if entry and entry.get("id")]
    if probe.get("id"):
        return [probe]
    return []


def is_playlist(probe: dict[str, Any]) -> bool:
    return probe.get("_type") == "playlist" or bool(probe.get("entries"))


def build_collection(probe: dict[str, Any], input_url: str) -> dict[str, Any]:
    playlist_id = probe.get("id") or probe.get("playlist_id") or "unknown"
    return {
        "type": "youtube_playlist",
        "external_id": playlist_id,
        "url": probe.get("webpage_url") or input_url,
        "title": probe.get("title") or playlist_id,
    }


def build_membership(entries: list[dict[str, Any]]) -> list[dict[str, Any]]:
    membership: list[dict[str, Any]] = []
    for position, entry in enumerate(entries):
        video_id = entry["id"]
        membership.append({"external_id": video_id, "position": position})
    return membership


def metadata_from_info_ytdlp(info: dict[str, Any]) -> dict[str, Any]:
    meta: dict[str, Any] = {}
    for key in (
        "title",
        "description",
        "channel",
        "channel_id",
        "channel_url",
        "uploader",
        "uploader_id",
        "uploader_url",
        "view_count",
        "like_count",
        "age_limit",
        "availability",
    ):
        if info.get(key) is not None:
            meta[key] = info[key]
    if info.get("channel_follower_count") is not None:
        meta["channel_follower_count"] = info["channel_follower_count"]
    if info.get("channel_is_verified") is not None:
        meta["channel_is_verified"] = info["channel_is_verified"]
    if upload_date := info.get("upload_date"):
        if len(upload_date) == 8:
            meta["upload_date"] = f"{upload_date[0:4]}-{upload_date[4:6]}-{upload_date[6:8]}"
        else:
            meta["upload_date"] = upload_date
    if duration := info.get("duration"):
        meta["duration"] = duration
    if categories := info.get("categories"):
        meta["categories"] = categories
    if tags := info.get("tags"):
        meta["tags"] = tags
    thumbnails = info.get("thumbnails") or []
    if thumbnails:
        meta["thumbnail_url"] = thumbnails[-1].get("url")
    if info.get("expected_height") is not None:
        meta["expected_height"] = info["expected_height"]
    if info.get("format_id"):
        meta["format_id"] = info["format_id"]
    return meta


def metadata_from_info(info: dict[str, Any], file_meta: dict[str, Any] | None = None) -> dict[str, Any]:
    """Legacy flat metadata for backward compatibility in tests."""
    meta = metadata_from_info_ytdlp(info)
    if file_meta:
        meta["file"] = file_meta
    if info.get("thumbnail_external_id"):
        meta["thumbnail_external_id"] = info["thumbnail_external_id"]
    return meta


def build_item(
    *,
    video_id: str,
    relative_path: str,
    status: str,
    info: dict[str, Any] | None,
    file_meta: dict[str, Any] | None = None,
) -> dict[str, Any]:
    item: dict[str, Any] = {
        "path": relative_path,
        "sha256": None,
        "status": status,
        "source_ref": {
            "source": "youtube",
            "kind": "video",
            "external_id": video_id,
            "url": info.get("webpage_url") if info else f"https://www.youtube.com/watch?v={video_id}",
        },
        "metadata_by_provenance": {},
    }
    if info:
        ytdlp_meta = metadata_from_info_ytdlp(info)
        if ytdlp_meta:
            item["metadata_by_provenance"]["yt-dlp"] = ytdlp_meta
        archiveos_meta: dict[str, Any] = {}
        if info.get("thumbnail_external_id"):
            archiveos_meta["thumbnail_external_id"] = info["thumbnail_external_id"]
        if archiveos_meta:
            item["metadata_by_provenance"]["archiveos"] = archiveos_meta
        if file_meta:
            item["metadata_by_provenance"]["ffprobe"] = file_meta
        item["metadata"] = metadata_from_info(info, file_meta)
    return item


def build_thumbnail_item(
    *,
    video_id: str,
    relative_path: str,
    info: dict[str, Any],
    source_thumbnail_url: str,
) -> dict[str, Any]:
    external_id = thumbnail_external_id(video_id)
    title = info.get("title") or video_id
    return {
        "path": relative_path,
        "sha256": None,
        "status": "complete",
        "source_ref": {
            "source": "youtube",
            "kind": "thumbnail",
            "external_id": external_id,
            "url": source_thumbnail_url,
        },
        "metadata_by_provenance": {
            "archiveos": {
                "entity_role": "supporting",
                "asset_role": "thumbnail",
                "visibility": "hidden",
                "thumbnail_for": video_id,
            },
            "yt-dlp": {
                "title": f"{title} thumbnail",
                "source_thumbnail_url": source_thumbnail_url,
            },
        },
        "metadata": {
            "title": f"{title} thumbnail",
            "thumbnail_for": video_id,
            "source_thumbnail_url": source_thumbnail_url,
        },
    }


def thumbnail_relation(video_id: str) -> dict[str, str]:
    return {
        "source": "youtube",
        "from_kind": "video",
        "from_external_id": video_id,
        "to_kind": "thumbnail",
        "to_external_id": thumbnail_external_id(video_id),
        "relation": "thumbnail",
    }


def with_thumbnail_metadata(info: dict[str, Any], video_id: str) -> dict[str, Any]:
    enriched = dict(info)
    enriched["thumbnail_external_id"] = thumbnail_external_id(video_id)
    return enriched


def build_manifest(
    *,
    vault_name: str,
    input_url: str,
    probe: dict[str, Any],
    items: list[dict[str, Any]],
    channels: list[dict[str, Any]],
    relations: list[dict[str, Any]],
) -> dict[str, Any]:
    entries = list_entries(probe)
    manifest: dict[str, Any] = {
        "source": "yt-dlp",
        "vault": vault_name,
        "strategy": "managed",
        "channels": channels,
        "items": items,
        "relations": relations,
    }
    if is_playlist(probe):
        manifest["collection"] = build_collection(probe, input_url)
        manifest["membership"] = build_membership(entries)
    return manifest


def relative_staging_path(files_dir: Path, file_path: Path) -> str:
    return f"files/{file_path.name}"
