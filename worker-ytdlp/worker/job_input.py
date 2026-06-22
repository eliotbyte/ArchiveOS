from __future__ import annotations

from .asset_policy import YtdlpJobInput

__all__ = ["YtdlpJobInput", "parse_job_input"]


def parse_job_input(raw: str) -> YtdlpJobInput:
    return YtdlpJobInput.parse(raw)
