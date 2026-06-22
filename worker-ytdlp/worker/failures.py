from __future__ import annotations

import re


def classify_error(message: str) -> tuple[str, bool]:
    text = message.lower()
    if "private video" in text or "sign in" in text or "members only" in text:
        return "private", False
    if "has been removed" in text or "video has been deleted" in text:
        return "dead", False
    if "video unavailable" in text or "not available" in text:
        return "unavailable", False
    if "geo" in text or "not made this video available in your country" in text:
        return "region_locked", False
    if "requested format is not available" in text or "no video formats" in text:
        return "no_formats", True
    if any(token in text for token in ("timed out", "connection", "network", "http error", "unable to download")):
        return "network", True
    if "validation" in text:
        return "validation_failed", True
    return "unknown", True


def error_kind_to_item_status(error_kind: str) -> str:
    """Map classified error to manifest item status understood by core import."""
    return {
        "private": "private",
        "dead": "dead",
        "unavailable": "unavailable",
        "region_locked": "region_locked",
    }.get(error_kind, "failed")
