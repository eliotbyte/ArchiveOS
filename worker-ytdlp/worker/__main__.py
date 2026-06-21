from __future__ import annotations

import logging
import shutil
import threading
import time
from pathlib import Path

from .channels import channel_from_info, uploaded_by_relation
from .config import Config
from .core_client import CoreClient
from .discovery import discover, require_entries
from .download import DownloadError, download_video, probe_video
from .failures import classify_error
from .manifest_builder import (
    build_item,
    build_manifest,
    build_thumbnail_item,
    relative_staging_path,
    thumbnail_relation,
    with_thumbnail_metadata,
)
from .thumbnails import (
    best_thumbnail_url,
    download_thumbnail,
    thumbnail_external_id,
)
from .updater import YtdlpUpdater, heartbeat_loop, update_ytdlp, ytdlp_version
from .validation import ValidationError, validate_download
from .ytdlp_runner import YtdlpError

logger = logging.getLogger(__name__)


class Worker:
    def __init__(self, config: Config) -> None:
        self.config = config
        self.client = CoreClient(config.core_url, config.vault_name)
        self.updater = YtdlpUpdater(
            enabled=config.ytdlp_auto_update,
            on_start=config.ytdlp_update_on_start,
            interval_secs=config.ytdlp_update_interval_secs,
        )
        self._job_active = False

    def run(self) -> None:
        logger.info(
            "worker starting vault=%s core=%s yt-dlp=%s",
            self.config.vault_name,
            self.config.core_url,
            ytdlp_version(),
        )
        self.updater.maybe_update(idle=True)
        while True:
            self.updater.maybe_update(idle=not self._job_active)
            job = self.client.claim_job("yt-dlp", self.config.job_lease_secs)
            if not job:
                time.sleep(self.config.job_poll_secs)
                continue
            self._job_active = True
            try:
                logger.info("yt-dlp version for job: %s", ytdlp_version())
                self.process_job(job)
            except Exception as err:
                logger.exception("job %s failed", job.get("id"))
                self.record_failure(
                    job_id=job.get("id"),
                    kind="video",
                    external_id="job",
                    url=job.get("input"),
                    stage="probe",
                    message=str(err),
                )
                self.submit_failure(job)
            finally:
                self._job_active = False

    def process_job(self, job: dict) -> None:
        job_id = job["id"]
        input_url = job["input"]
        staging_dir = Path(self.config.vault_path) / "staging" / job_id
        files_dir = staging_dir / "files"
        if staging_dir.exists():
            shutil.rmtree(staging_dir)
        files_dir.mkdir(parents=True, exist_ok=True)

        stop = threading.Event()

        def send_heartbeat() -> None:
            self.client.heartbeat(job_id, self.config.job_lease_secs)

        heartbeat_thread = threading.Thread(
            target=heartbeat_loop,
            kwargs={
                "job_id": job_id,
                "lease_secs": self.config.job_lease_secs,
                "send_heartbeat": send_heartbeat,
                "stop": stop.is_set,
            },
            daemon=True,
        )
        heartbeat_thread.start()

        try:
            probe = discover(
                input_url,
                playlist_max_items=self.config.ytdlp_playlist_max_items,
            )
            video_ids = require_entries(probe)
            present = self.client.sources_has(video_ids, kind="video")
            missing = [video_id for video_id in video_ids if not present.get(video_id)]
            thumb_ids = [thumbnail_external_id(video_id) for video_id in video_ids]
            thumbs_present = self.client.sources_has(thumb_ids, kind="thumbnail")
            missing_thumbs = [
                video_id
                for video_id in video_ids
                if present.get(video_id) and not thumbs_present.get(thumbnail_external_id(video_id))
            ]

            logger.info(
                "job %s entries=%d missing_videos=%d missing_thumbs=%d",
                job_id,
                len(video_ids),
                len(missing),
                len(missing_thumbs),
            )

            items: list[dict] = []
            channels: dict[str, dict] = {}
            relations: list[dict] = []

            for index, video_id in enumerate(missing, start=1):
                logger.info("job %s acquiring %d/%d %s", job_id, index, len(missing), video_id)
                acquired_items, channel, acquired_relations = self.acquire_video(
                    job_id=job_id,
                    video_id=video_id,
                    files_dir=files_dir,
                )
                items.extend(acquired_items)
                if channel:
                    channels[channel["external_id"]] = channel
                relations.extend(acquired_relations)

            for index, video_id in enumerate(missing_thumbs, start=1):
                logger.info(
                    "job %s thumbnail-only %d/%d %s",
                    job_id,
                    index,
                    len(missing_thumbs),
                    video_id,
                )
                thumb_items, acquired_relations = self.acquire_thumbnail_only(
                    job_id=job_id,
                    video_id=video_id,
                    files_dir=files_dir,
                )
                items.extend(thumb_items)
                relations.extend(acquired_relations)

            manifest = build_manifest(
                vault_name=self.config.vault_name,
                input_url=input_url,
                probe=probe,
                items=items,
                channels=list(channels.values()),
                relations=relations,
            )
            status = self.resolve_status(items)
            result = self.client.submit_manifest(job_id, manifest, status=status)
            logger.info("job %s finished: %s", job_id, result)
        finally:
            stop.set()
            heartbeat_thread.join(timeout=2)

    def acquire_video(
        self,
        *,
        job_id: str,
        video_id: str,
        files_dir: Path,
    ) -> tuple[list[dict], dict | None, list[dict]]:
        url = f"https://www.youtube.com/watch?v={video_id}"
        relations: list[dict] = []
        try:
            file_path, info, error = download_video(video_id, files_dir)
            if error:
                self.record_failure(
                    job_id=job_id,
                    kind="video",
                    external_id=video_id,
                    url=url,
                    stage="download",
                    message=error,
                )
                return (
                    [
                        build_item(
                            video_id=video_id,
                            relative_path=f"files/{video_id}.mp4",
                            status="failed",
                            info=info,
                        )
                    ],
                    channel_from_info(info) if info else None,
                    relations,
                )

            assert file_path is not None
            assert info is not None
            _, file_meta = validate_download(file_path=file_path, info=info)
            file_meta["ytdlp_version"] = ytdlp_version()
            channel = channel_from_info(info)
            if channel:
                relations.append(uploaded_by_relation(video_id, channel["external_id"]))

            items = [
                build_item(
                    video_id=video_id,
                    relative_path=relative_staging_path(files_dir, file_path),
                    status="complete",
                    info=with_thumbnail_metadata(info, video_id),
                    file_meta=file_meta,
                )
            ]

            thumb_item = self.try_thumbnail(
                job_id=job_id,
                video_id=video_id,
                info=info,
                files_dir=files_dir,
            )
            if thumb_item:
                items.append(thumb_item)
                relations.append(thumbnail_relation(video_id))

            return items, channel, relations
        except (DownloadError, ValidationError, YtdlpError) as err:
            stage = "validate" if isinstance(err, ValidationError) else "download"
            self.record_failure(
                job_id=job_id,
                kind="video",
                external_id=video_id,
                url=url,
                stage=stage,
                message=str(err),
            )
            return (
                [
                    build_item(
                        video_id=video_id,
                        relative_path=f"files/{video_id}.mp4",
                        status="failed",
                        info=None,
                    )
                ],
                None,
                relations,
            )

    def acquire_thumbnail_only(
        self,
        *,
        job_id: str,
        video_id: str,
        files_dir: Path,
    ) -> tuple[list[dict], list[dict]]:
        relations: list[dict] = []
        try:
            info = probe_video(video_id)
        except DownloadError as err:
            self.record_failure(
                job_id=job_id,
                kind="thumbnail",
                external_id=thumbnail_external_id(video_id),
                url=f"https://www.youtube.com/watch?v={video_id}",
                stage="download",
                message=str(err),
            )
            return [], relations

        thumb_item = self.try_thumbnail(
            job_id=job_id,
            video_id=video_id,
            info=info,
            files_dir=files_dir,
        )
        if thumb_item:
            relations.append(thumbnail_relation(video_id))
            return [thumb_item], relations
        return [], relations

    def try_thumbnail(
        self,
        *,
        job_id: str,
        video_id: str,
        info: dict,
        files_dir: Path,
    ) -> dict | None:
        source_url = best_thumbnail_url(info)
        thumb_path, error = download_thumbnail(video_id, info, files_dir)
        if error or thumb_path is None:
            self.record_failure(
                job_id=job_id,
                kind="thumbnail",
                external_id=thumbnail_external_id(video_id),
                url=source_url,
                stage="download",
                message=error or "thumbnail download failed",
            )
            return None

        return build_thumbnail_item(
            video_id=video_id,
            relative_path=relative_staging_path(files_dir, thumb_path),
            info=info,
            source_thumbnail_url=source_url or "",
        )

    def record_failure(
        self,
        *,
        job_id: str | None,
        kind: str,
        external_id: str,
        url: str | None,
        stage: str,
        message: str,
    ) -> None:
        error_kind, retryable = classify_error(message)
        try:
            self.client.record_failure(
                job_id=job_id,
                source="youtube",
                kind=kind,
                external_id=external_id,
                url=url,
                stage=stage,
                error_kind=error_kind,
                message=message,
                retryable=retryable,
            )
        except Exception:
            logger.exception("failed to persist source failure for %s", external_id)

    def resolve_status(self, items: list[dict]) -> str | None:
        video_items = [
            item
            for item in items
            if (item.get("source_ref") or {}).get("kind") == "video"
        ]
        if not video_items:
            return "done"
        complete = sum(1 for item in video_items if item.get("status") == "complete")
        failed = sum(1 for item in video_items if item.get("status") == "failed")
        if failed > 0 and complete > 0:
            return "partial"
        if failed > 0 and complete == 0:
            return "failed"
        return "done"

    def submit_failure(self, job: dict) -> None:
        job_id = job["id"]
        manifest = {
            "source": "yt-dlp",
            "vault": self.config.vault_name,
            "strategy": "managed",
            "items": [],
            "channels": [],
            "relations": [],
        }
        try:
            self.client.submit_manifest(job_id, manifest, status="failed")
        except Exception:
            logger.exception("failed to mark job %s failed", job_id)


def main() -> None:
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s %(message)s",
    )
    config = Config.from_env()
    if config.ytdlp_update_on_start:
        update_ytdlp()
    Worker(config).run()


if __name__ == "__main__":
    main()
