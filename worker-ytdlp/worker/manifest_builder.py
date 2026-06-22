from __future__ import annotations

from pathlib import Path
from typing import Any

from .source_mapper import (
    collection_type,
    extractor_from_info,
    extractor_from_probe,
    is_channel_uploads,
    video_url_from_info,
)
from .thumbnails import thumbnail_external_id
from .track_catalog import build_localized_text, build_track_catalog
from .asset_policy import AssetPolicy, filter_track_catalog


def list_entries(probe: dict[str, Any]) -> list[dict[str, Any]]:
    if probe.get("_type") == "playlist" or probe.get("entries"):
        entries = probe.get("entries") or []
        return [entry for entry in entries if entry and entry.get("id")]
    if probe.get("id"):
        return [probe]
    return []


def is_playlist(probe: dict[str, Any]) -> bool:
    return probe.get("_type") == "playlist" or bool(probe.get("entries"))


def build_collection(probe: dict[str, Any], input_url: str, source: str) -> dict[str, Any]:
    playlist_id = probe.get("id") or probe.get("playlist_id") or "unknown"
    return {
        "type": collection_type(source, probe, input_url),
        "external_id": playlist_id,
        "url": probe.get("webpage_url") or input_url,
        "title": probe.get("title") or playlist_id,
    }


def channel_from_probe(probe: dict[str, Any], source: str) -> dict[str, Any] | None:
    channel_id = probe.get("channel_id")
    uploader_id = probe.get("uploader_id")
    external_id = channel_id or uploader_id
    if not external_id:
        return None
    metadata: dict[str, Any] = {}
    if title := probe.get("channel") or probe.get("uploader") or probe.get("title"):
        metadata["title"] = title
    description = probe.get("channel_description") or probe.get("description")
    if description:
        metadata["description"] = description
    return {
        "source": source,
        "kind": "channel",
        "external_id": external_id,
        "url": probe.get("channel_url") or probe.get("uploader_url"),
        "metadata": metadata or None,
    }


def build_membership(entries: list[dict[str, Any]]) -> list[dict[str, Any]]:
    membership: list[dict[str, Any]] = []
    for position, entry in enumerate(entries):
        membership.append(
            {
                "external_id": entry["id"],
                "position": position,
                "kind": "video",
                "url": entry.get("webpage_url") or entry.get("url"),
            }
        )
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
        "language",
        "alt_title",
    ):
        value = info.get(key)
        if value is None:
            continue
        if isinstance(value, str) and not value and key in {"title", "description"}:
            continue
        meta[key] = value
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
    if cast := info.get("cast"):
        meta["cast"] = cast
    thumbnails = info.get("thumbnails") or []
    if thumbnails:
        meta["thumbnail_url"] = thumbnails[-1].get("url")
    if info.get("expected_height") is not None:
        meta["expected_height"] = info["expected_height"]
    if info.get("format_id"):
        meta["format_id"] = info["format_id"]
    localized = build_localized_text(info)
    if localized:
        meta["localized_text"] = localized
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
    source: str | None = None,
    asset_policy: AssetPolicy | None = None,
) -> dict[str, Any]:
    resolved_source = source or (extractor_from_info(info) if info else "unknown")
    item: dict[str, Any] = {
        "path": relative_path,
        "sha256": None,
        "status": status,
        "source_ref": {
            "source": resolved_source,
            "kind": "video",
            "external_id": video_id,
            "url": video_url_from_info(info, video_id) if info else None,
        },
        "metadata_by_provenance": {},
        "assets": [],
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
        catalog = build_track_catalog(info, video_id)
        if asset_policy is not None:
            catalog = filter_track_catalog(
                catalog,
                asset_policy,
                info_language=str(info.get("language") or "") or None,
            )
        item["assets"] = catalog
    return item


def build_discovered_item(
    entry: dict[str, Any],
    source: str | None = None,
    asset_policy: AssetPolicy | None = None,
) -> dict[str, Any]:
    video_id = entry["id"]
    resolved_source = source or extractor_from_info(entry)
    metadata = metadata_from_info_ytdlp(entry)
    item: dict[str, Any] = {
        "path": "",
        "sha256": None,
        "status": "discovered",
        "source_ref": {
            "source": resolved_source,
            "kind": "video",
            "external_id": video_id,
            "url": entry.get("webpage_url") or entry.get("url"),
        },
        "metadata_by_provenance": {},
        "assets": _filtered_catalog(entry, video_id, asset_policy),
    }
    if metadata:
        item["metadata_by_provenance"]["yt-dlp"] = metadata
        item["metadata"] = metadata
    return item


def _filtered_catalog(
    entry: dict[str, Any],
    video_id: str,
    asset_policy: AssetPolicy | None,
) -> list[dict[str, Any]]:
    catalog = build_track_catalog(entry, video_id)
    if asset_policy is None:
        return catalog
    return filter_track_catalog(
        catalog,
        asset_policy,
        info_language=str(entry.get("language") or "") or None,
    )


def build_thumbnail_item(
    *,
    video_id: str,
    relative_path: str,
    info: dict[str, Any],
    source_thumbnail_url: str,
    source: str | None = None,
) -> dict[str, Any]:
    resolved_source = source or extractor_from_info(info)
    external_id = thumbnail_external_id(video_id)
    title = info.get("title") or video_id
    return {
        "path": relative_path,
        "sha256": None,
        "status": "complete",
        "source_ref": {
            "source": resolved_source,
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
                "preview_role": "source_thumbnail",
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
        "assets": [],
    }


def thumbnail_relation(video_id: str, source: str) -> dict[str, str]:
    return {
        "source": source,
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
    source: str | None = None,
    asset_policy: AssetPolicy | None = None,
) -> dict[str, Any]:
    resolved_source = source or extractor_from_probe(probe)
    entries = list_entries(probe)
    item_ids = {
        item["source_ref"]["external_id"]
        for item in items
        if item.get("source_ref") and item["source_ref"].get("kind") == "video"
    }
    discovery_items = [
        build_discovered_item(entry, resolved_source, asset_policy)
        for entry in entries
        if entry["id"] not in item_ids
    ]
    channels_by_id = {
        channel["external_id"]: channel
        for channel in channels
        if channel.get("external_id")
    }
    if channel := channel_from_probe(probe, resolved_source):
        channels_by_id[channel["external_id"]] = channel
    manifest: dict[str, Any] = {
        "source": "yt-dlp",
        "source_identity": resolved_source,
        "vault": vault_name,
        "strategy": "managed",
        "channels": list(channels_by_id.values()),
        "items": [*items, *discovery_items],
        "relations": relations,
    }
    if is_playlist(probe):
        manifest["collection"] = build_collection(probe, input_url, resolved_source)
        manifest["membership"] = build_membership(entries)
    return manifest


def relative_staging_path(files_dir: Path, file_path: Path) -> str:
    return f"files/{file_path.name}"
