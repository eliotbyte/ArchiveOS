from __future__ import annotations

import logging
import os
import time
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)


def staging_file_path(staging_dir: Path, relative_path: str) -> Path:
    return staging_dir / relative_path


def staging_file_exists(staging_dir: Path, relative_path: str) -> bool:
    if not relative_path:
        return False
    return staging_file_path(staging_dir, relative_path).is_file()


def channel_avatar_in_staging(channel: dict[str, Any], staging_dir: Path) -> bool:
    avatar = channel.get("avatar")
    if not isinstance(avatar, dict):
        return False
    path = avatar.get("path")
    if not isinstance(path, str) or not path:
        return False
    return staging_file_exists(staging_dir, path)


def flush_staging(staging_dir: Path) -> None:
    """Best-effort fsync so bind-mounted volumes see new files in other containers."""
    try:
        os.sync()
    except AttributeError:
        pass
    files_dir = staging_dir / "files"
    if not files_dir.is_dir():
        return
    for path in files_dir.iterdir():
        if not path.is_file():
            continue
        try:
            with path.open("rb") as handle:
                os.fsync(handle.fileno())
        except OSError:
            continue


def wait_for_staging_files(
    staging_dir: Path,
    relative_paths: list[str],
    *,
    timeout_secs: float = 12.0,
    interval_secs: float = 0.5,
) -> list[str]:
    pending = [path for path in relative_paths if path]
    if not pending:
        return []
    deadline = time.monotonic() + timeout_secs
    while pending and time.monotonic() < deadline:
        pending = [
            path for path in pending if not staging_file_exists(staging_dir, path)
        ]
        if pending:
            time.sleep(interval_secs)
    return pending


def manifest_staging_paths(manifest: dict[str, Any]) -> list[str]:
    paths: list[str] = []
    for item in manifest.get("items") or []:
        if item.get("status") == "complete" and item.get("path"):
            paths.append(str(item["path"]))
    for channel in manifest.get("channels") or []:
        avatar = channel.get("avatar")
        if isinstance(avatar, dict) and avatar.get("path"):
            paths.append(str(avatar["path"]))
    return paths


def sanitize_manifest_for_staging(
    manifest: dict[str, Any],
    staging_dir: Path,
) -> dict[str, Any]:
    items: list[dict[str, Any]] = []
    for item in manifest.get("items") or []:
        row = dict(item)
        if row.get("status") == "complete" and row.get("path"):
            if not staging_file_exists(staging_dir, str(row["path"])):
                external_id = (row.get("source_ref") or {}).get("external_id")
                logger.warning(
                    "dropping missing staging file for item %s: %s",
                    external_id,
                    row["path"],
                )
                row["status"] = "failed"
        items.append(row)

    channels: list[dict[str, Any]] = []
    for channel in manifest.get("channels") or []:
        row = dict(channel)
        if row.get("avatar") and not channel_avatar_in_staging(row, staging_dir):
            logger.warning(
                "dropping missing channel avatar for %s",
                row.get("external_id"),
            )
            row.pop("avatar", None)
        channels.append(row)

    return {**manifest, "items": items, "channels": channels}
