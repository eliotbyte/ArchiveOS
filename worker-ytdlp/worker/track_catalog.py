from __future__ import annotations

import json
from typing import Any


def _subtitle_track_key(lang: str, ext: str, caption_kind: str) -> str:
    return f"subtitle:{lang}:{ext}:{caption_kind}"


def _audio_track_key(language: str, format_id: str) -> str:
    return f"audio:{language}:{format_id}"


def build_subtitle_catalog(info: dict[str, Any], video_id: str) -> list[dict[str, Any]]:
    assets: list[dict[str, Any]] = []
    source_page_url = info.get("webpage_url")
    for caption_kind, subs in (
        ("manual", info.get("subtitles") or {}),
        ("automatic", info.get("automatic_captions") or {}),
    ):
        if not isinstance(subs, dict):
            continue
        for lang, formats in subs.items():
            if not isinstance(formats, list):
                continue
            for fmt in formats:
                if not isinstance(fmt, dict):
                    continue
                ext = str(fmt.get("ext") or "unknown")
                track_key = _subtitle_track_key(str(lang), ext, caption_kind)
                metadata: dict[str, Any] = {
                    "language": lang,
                    "ext": ext,
                    "caption_kind": caption_kind,
                }
                if name := fmt.get("name"):
                    metadata["name"] = name
                if source_url := fmt.get("url"):
                    metadata["source_url"] = source_url
                if source_page_url:
                    metadata["source_page_url"] = source_page_url
                if http_headers := fmt.get("http_headers"):
                    if isinstance(http_headers, dict):
                        metadata["http_headers"] = json.dumps(http_headers)
                    else:
                        metadata["http_headers"] = http_headers
                assets.append(
                    {
                        "track_key": track_key,
                        "role": "supporting",
                        "kind": "subtitle",
                        "status": "remote",
                        "storage_strategy": "remote",
                        "external_id": f"{video_id}:{track_key}",
                        "metadata": metadata,
                    }
                )
    return assets


def build_audio_catalog(info: dict[str, Any], video_id: str) -> list[dict[str, Any]]:
    assets: list[dict[str, Any]] = []
    seen: set[str] = set()
    source_page_url = info.get("webpage_url")
    for fmt in info.get("formats") or []:
        if not isinstance(fmt, dict):
            continue
        acodec = fmt.get("acodec")
        vcodec = fmt.get("vcodec")
        if acodec in (None, "none"):
            continue
        if vcodec not in (None, "none") and acodec not in (None, "none"):
            # Combined av stream; still catalog distinct audio languages when present.
            pass
        language = str(fmt.get("language") or "und")
        format_id = str(fmt.get("format_id") or "unknown")
        track_key = _audio_track_key(language, format_id)
        if track_key in seen:
            continue
        seen.add(track_key)
        metadata: dict[str, Any] = {
            "language": language,
            "format_id": format_id,
        }
        for field in ("format_note", "acodec", "abr", "asr", "audio_channels"):
            if fmt.get(field) is not None:
                metadata[field] = fmt[field]
        if fmt.get("language_preference") is not None:
            metadata["language_preference"] = fmt["language_preference"]
        if fmt.get("url"):
            metadata["source_url"] = fmt["url"]
        if source_page_url:
            metadata["source_page_url"] = source_page_url
        if http_headers := fmt.get("http_headers"):
            if isinstance(http_headers, dict):
                metadata["http_headers"] = json.dumps(http_headers)
            else:
                metadata["http_headers"] = http_headers
        assets.append(
            {
                "track_key": track_key,
                "role": "supporting",
                "kind": "audio",
                "status": "remote",
                "storage_strategy": "remote",
                "external_id": f"{video_id}:{track_key}",
                "metadata": metadata,
            }
        )
    return assets


def build_track_catalog(info: dict[str, Any], video_id: str) -> list[dict[str, Any]]:
    return [*build_subtitle_catalog(info, video_id), *build_audio_catalog(info, video_id)]


def build_localized_text(info: dict[str, Any]) -> list[dict[str, Any]]:
    entries: list[dict[str, Any]] = []
    title = info.get("title")
    if isinstance(title, str) and title:
        entries.append(
            {
                "field": "title",
                "language": info.get("language") or "und",
                "value": title,
                "is_primary": True,
                "is_translated": False,
            }
        )
    alt_title = info.get("alt_title")
    if isinstance(alt_title, str) and alt_title and alt_title != title:
        entries.append(
            {
                "field": "title",
                "language": "und",
                "value": alt_title,
                "is_primary": False,
                "is_translated": False,
            }
        )
    description = info.get("description")
    if isinstance(description, str) and description:
        entries.append(
            {
                "field": "description",
                "language": info.get("language") or "und",
                "value": description,
                "is_primary": True,
                "is_translated": False,
            }
        )
    return entries
