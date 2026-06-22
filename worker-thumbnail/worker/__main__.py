from __future__ import annotations

import json
import logging
import os
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path

import requests
from PIL import Image

logger = logging.getLogger(__name__)

ROLE_SMALL = "preview_image_small"
ROLE_LARGE = "preview_image_large"
ROLE_SPRITE = "timeline_sprite"
ROLE_MANIFEST = "timeline_manifest"

SUPPORTED_PREFIXES = ("image/", "video/")


@dataclass(frozen=True)
class Config:
    core_url: str
    vault_name: str
    vault_path: Path
    job_poll_secs: float
    job_lease_secs: int

    @classmethod
    def from_env(cls) -> "Config":
        return cls(
            core_url=os.environ.get("CORE_URL", "http://localhost:8080").rstrip("/"),
            vault_name=os.environ["VAULT_NAME"],
            vault_path=Path(os.environ["VAULT_PATH"]),
            job_poll_secs=float(os.environ.get("JOB_POLL_SECS", "5")),
            job_lease_secs=int(os.environ.get("JOB_LEASE_SECS", "600")),
        )


class CoreClient:
    def __init__(self, config: Config) -> None:
        self.config = config
        self.session = requests.Session()

    def claim_job(self) -> dict | None:
        response = self.session.post(
            f"{self.config.core_url}/vaults/{self.config.vault_name}/jobs/claim",
            json={"type": "preview", "lease_secs": self.config.job_lease_secs},
            timeout=30,
        )
        response.raise_for_status()
        return response.json()

    def get_entity(self, entity_id: str) -> dict:
        response = self.session.get(
            f"{self.config.core_url}/vaults/{self.config.vault_name}/entities/{entity_id}",
            timeout=30,
        )
        response.raise_for_status()
        return response.json()

    def commit_preview(self, job_id: str, entity_id: str, files: list[dict]) -> None:
        self.session.post(
            f"{self.config.core_url}/vaults/{self.config.vault_name}/jobs/{job_id}/preview-commit",
            json={"entity_id": entity_id, "files": files},
            timeout=120,
        ).raise_for_status()

    def finish_job(self, job_id: str, *, status: str = "failed") -> None:
        self.session.post(
            f"{self.config.core_url}/vaults/{self.config.vault_name}/jobs/{job_id}/finish",
            json={"status": status},
            timeout=30,
        ).raise_for_status()


def primary_asset(entity: dict) -> dict | None:
    for asset in entity.get("assets", []):
        if asset.get("role") == "primary" and asset.get("status") == "present":
            return asset
    return None


def mime_extension(mime: str) -> str | None:
    mapping = {
        "video/mp4": "mp4",
        "video/webm": "webm",
        "image/jpeg": "jpg",
        "image/png": "png",
        "image/webp": "webp",
        "image/gif": "gif",
    }
    if mime in mapping:
        return mapping[mime]
    if "/" in mime:
        subtype = mime.split("/", 1)[1]
        if subtype == "jpeg":
            return "jpg"
        return subtype
    return None


def managed_blob_path(asset: dict, vault_path: Path) -> Path | None:
    content_hash = asset.get("content_hash")
    if not content_hash or len(content_hash) != 64:
        return None

    candidates: list[str] = []
    ext = (asset.get("ext") or "").lstrip(".")
    if ext:
        candidates.append(ext)
    mime_ext = mime_extension((asset.get("mime") or "").lower())
    if mime_ext and mime_ext not in candidates:
        candidates.append(mime_ext)

    shard = vault_path / "blobs" / content_hash[:2] / content_hash[2:4]
    for candidate in candidates:
        blob = shard / f"{content_hash}.{candidate}"
        if blob.is_file():
            return blob

    if not shard.is_dir():
        return None

    prefix = f"{content_hash}."
    for child in shard.iterdir():
        if not child.is_file():
            continue
        name = child.name
        if name == content_hash or name.startswith(prefix):
            return child
    return None


def source_path(asset: dict, vault_path: Path) -> Path | None:
    path = asset.get("path")
    if path:
        candidate = Path(path)
        if candidate.is_file():
            return candidate
        relative = vault_path / path.lstrip("/\\")
        if relative.is_file():
            return relative
        if "blobs/" in path.replace("\\", "/"):
            blob = vault_path / "blobs" / path.replace("\\", "/").split("blobs/", 1)[1]
            if blob.is_file():
                return blob
        if candidate.exists():
            return candidate

    return managed_blob_path(asset, vault_path)


def is_supported_mime(mime: str) -> bool:
    return mime.startswith(SUPPORTED_PREFIXES)


def write_webp(source: Path, target: Path, size: tuple[int, int]) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    with Image.open(source) as image:
        image = image.convert("RGB")
        image.thumbnail(size, Image.Resampling.LANCZOS)
        image.save(target, format="WEBP", quality=82)


def generate_image_previews(source: Path, staging: Path) -> list[dict]:
    small = staging / "files" / "preview_small.webp"
    large = staging / "files" / "preview_large.webp"
    write_webp(source, small, (320, 180))
    write_webp(source, large, (1280, 720))
    return [
        {
            "path": "files/preview_small.webp",
            "preview_role": ROLE_SMALL,
            "kind": "preview",
            "mime": "image/webp",
        },
        {
            "path": "files/preview_large.webp",
            "preview_role": ROLE_LARGE,
            "kind": "preview",
            "mime": "image/webp",
        },
    ]


def probe_duration(source: Path) -> float:
    result = subprocess.run(
        [
            "ffprobe",
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            str(source),
        ],
        capture_output=True,
        text=True,
        check=True,
    )
    return max(1.0, float(result.stdout.strip() or "1"))


def extract_frame(source: Path, target: Path, timestamp: float) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            "ffmpeg",
            "-hide_banner",
            "-loglevel",
            "error",
            "-ss",
            str(timestamp),
            "-i",
            str(source),
            "-frames:v",
            "1",
            "-q:v",
            "4",
            str(target),
            "-y",
        ],
        check=True,
    )


def generate_video_previews(source: Path, staging: Path) -> list[dict]:
    duration = probe_duration(source)
    frame_count = 12
    tile_w, tile_h = 160, 90
    columns = 4
    rows = 3
    frames_dir = staging / "files" / "frames"
    frames_dir.mkdir(parents=True, exist_ok=True)

    manifest_frames = []
    sprites: list[Image.Image] = []
    for index in range(frame_count):
        timestamp = (duration / frame_count) * index
        frame_path = frames_dir / f"frame_{index:02d}.jpg"
        extract_frame(source, frame_path, timestamp)
        with Image.open(frame_path) as frame_image:
            sprites.append(frame_image.convert("RGB").resize((tile_w, tile_h)))

        start = timestamp
        end = (duration / frame_count) * (index + 1)
        manifest_frames.append(
            {"index": index, "start_secs": round(start, 3), "end_secs": round(end, 3)},
        )

    sheet = Image.new("RGB", (tile_w * columns, tile_h * rows))
    for index, tile in enumerate(sprites):
        x = (index % columns) * tile_w
        y = (index // columns) * tile_h
        sheet.paste(tile, (x, y))
        tile.close()

    sprite_path = staging / "files" / "timeline_sprite.webp"
    manifest_path = staging / "files" / "timeline_manifest.json"
    large_path = staging / "files" / "preview_large.webp"
    small_path = staging / "files" / "preview_small.webp"
    sheet.save(sprite_path, format="WEBP", quality=80)
    sheet.close()
    poster_frame = frames_dir / "frame_00.jpg"
    write_webp(poster_frame, small_path, (320, 180))
    write_webp(poster_frame, large_path, (1280, 720))

    manifest = {
        "version": 1,
        "tile_width": tile_w,
        "tile_height": tile_h,
        "columns": columns,
        "rows": rows,
        "duration_secs": round(duration, 3),
        "frames": manifest_frames,
    }
    manifest_path.write_text(json.dumps(manifest), encoding="utf-8")

    return [
        {
            "path": "files/preview_small.webp",
            "preview_role": ROLE_SMALL,
            "kind": "preview",
            "mime": "image/webp",
        },
        {
            "path": "files/preview_large.webp",
            "preview_role": ROLE_LARGE,
            "kind": "preview",
            "mime": "image/webp",
        },
        {
            "path": "files/timeline_sprite.webp",
            "preview_role": ROLE_SPRITE,
            "kind": "preview",
            "mime": "image/webp",
        },
        {
            "path": "files/timeline_manifest.json",
            "preview_role": ROLE_MANIFEST,
            "kind": "manifest",
            "mime": "application/json",
        },
    ]


def process_job(client: CoreClient, job: dict, config: Config) -> None:
    job_id = job["id"]
    payload = json.loads(job["input"])
    entity_id = payload["entity_id"]
    entity = client.get_entity(entity_id)
    asset = primary_asset(entity)
    if not asset:
        raise RuntimeError(f"entity {entity_id} has no present primary asset")

    mime = (asset.get("mime") or "").lower()
    if not is_supported_mime(mime):
        raise RuntimeError(f"unsupported primary mime for preview generation: {mime or 'unknown'}")

    source = source_path(asset, config.vault_path)
    if not source or not source.is_file():
        raise RuntimeError(f"primary asset file missing for entity {entity_id}")

    staging = config.vault_path / "staging" / job_id
    if staging.exists():
        for child in staging.rglob("*"):
            if child.is_file():
                child.unlink()
    staging.mkdir(parents=True, exist_ok=True)

    if mime.startswith("video/"):
        files = generate_video_previews(source, staging)
    else:
        files = generate_image_previews(source, staging)

    logger.info(
        "preview generated entity_id=%s mime=%s files=%s",
        entity_id,
        mime,
        [item["preview_role"] for item in files],
    )
    client.commit_preview(job_id, entity_id, files)


def main() -> None:
    logging.basicConfig(level=logging.INFO)
    config = Config.from_env()
    client = CoreClient(config)
    logger.info("thumbnail worker starting vault=%s", config.vault_name)

    while True:
        job = client.claim_job()
        if not job:
            time.sleep(config.job_poll_secs)
            continue
        job_id = job["id"]
        try:
            process_job(client, job, config)
            logger.info("preview job done id=%s", job_id)
        except Exception as exc:
            logger.exception("preview job failed id=%s reason=%s", job_id, exc)
            try:
                client.finish_job(job_id, status="failed")
            except Exception:
                logger.exception("failed to mark job failed id=%s", job_id)


if __name__ == "__main__":
    main()
