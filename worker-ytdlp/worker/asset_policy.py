from __future__ import annotations

import json
from dataclasses import dataclass, field
from typing import Any


@dataclass
class AssetPolicy:
    video: str = "best"
    thumbnail: bool = True
    channel_avatar: bool = True
    subtitles: str = "preferred"
    subtitle_languages: list[str] = field(
        default_factory=lambda: ["original", "en", "ru"]
    )
    automatic_subtitles: bool = True
    audio_tracks: str = "main"
    audio_languages: list[str] = field(default_factory=list)

    @classmethod
    def from_dict(cls, raw: dict[str, Any] | None) -> AssetPolicy:
        if not raw:
            return cls()
        return cls(
            video=str(raw.get("video") or "best"),
            thumbnail=bool(raw.get("thumbnail", True)),
            channel_avatar=bool(raw.get("channel_avatar", True)),
            subtitles=str(raw.get("subtitles") or "preferred"),
            subtitle_languages=list(raw.get("subtitle_languages") or ["original", "en", "ru"]),
            automatic_subtitles=bool(raw.get("automatic_subtitles", True)),
            audio_tracks=str(raw.get("audio_tracks") or "main"),
            audio_languages=list(raw.get("audio_languages") or []),
        )


@dataclass
class YtdlpJobInput:
    url: str
    mode: str = "once"
    resync: bool = True
    removed_items: str = "mark_removed"
    asset_policy: AssetPolicy = field(default_factory=AssetPolicy)

    @classmethod
    def parse(cls, raw: str) -> YtdlpJobInput:
        trimmed = raw.strip()
        if trimmed.startswith("{"):
            payload = json.loads(trimmed)
            return cls(
                url=str(payload["url"]),
                mode=str(payload.get("mode") or "once"),
                resync=bool(payload.get("resync", True)),
                removed_items=str(payload.get("removed_items") or "mark_removed"),
                asset_policy=AssetPolicy.from_dict(payload.get("asset_policy")),
            )
        return cls(url=trimmed)


def video_format_selector(policy: AssetPolicy) -> str | None:
    if policy.video == "none":
        return None
    if policy.video == "best_1080p":
        return "bv*[height<=1080]+ba/b[height<=1080]/b"
    if policy.video == "audio_only":
        return "ba/b"
    return "bv*+ba/b"


def should_download_video(policy: AssetPolicy) -> bool:
    return policy.video != "none"


def is_metadata_only_refresh(policy: AssetPolicy) -> bool:
    return policy.video == "none"


def should_download_thumbnail(policy: AssetPolicy) -> bool:
    return policy.thumbnail


def should_download_channel_avatar(policy: AssetPolicy) -> bool:
    return policy.channel_avatar


def _language_matches(requested: str, candidate: str, info_language: str | None) -> bool:
    if requested == "original":
        if info_language and candidate not in {"und", "unknown"}:
            return candidate == info_language or candidate.startswith(info_language.split("-")[0])
        return candidate not in {"und", "unknown"}
    return candidate == requested or candidate.startswith(requested.split("-")[0])


def filter_track_catalog(
    assets: list[dict[str, Any]],
    policy: AssetPolicy,
    *,
    info_language: str | None = None,
) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    for asset in assets:
        kind = asset.get("kind")
        metadata = asset.get("metadata") or {}
        if kind == "subtitle":
            if policy.subtitles == "none":
                continue
            caption_kind = metadata.get("caption_kind", "manual")
            if policy.subtitles == "manual" and caption_kind != "manual":
                continue
            if policy.subtitles in {"manual_auto", "preferred"} and caption_kind == "automatic":
                if not policy.automatic_subtitles and policy.subtitles != "preferred":
                    continue
            if policy.subtitles == "preferred":
                lang = str(metadata.get("language") or "und")
                if policy.subtitle_languages and not any(
                    _language_matches(req, lang, info_language)
                    for req in policy.subtitle_languages
                ):
                    continue
            result.append(asset)
            continue
        if kind == "audio":
            if policy.audio_tracks == "none":
                continue
            if policy.audio_tracks == "main":
                pref = metadata.get("language_preference")
                if pref not in (None, 1, "1", True):
                    continue
            if policy.audio_tracks == "preferred" and policy.audio_languages:
                lang = str(metadata.get("language") or "und")
                if not any(
                    _language_matches(req, lang, info_language)
                    for req in policy.audio_languages
                ):
                    continue
            result.append(asset)
    return result
