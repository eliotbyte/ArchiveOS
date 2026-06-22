from __future__ import annotations

import json
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
from .asset_policy import (
    should_download_thumbnail,
    should_download_video,
    video_format_selector,
)
from .job_input import parse_job_input
from .ytdlp_args import cookies_args
from .failures import classify_error, error_kind_to_item_status
from .manifest_builder import (
    build_item,
    build_manifest,
    build_thumbnail_item,
    relative_staging_path,
    thumbnail_relation,
    with_thumbnail_metadata,
)
from .source_mapper import extractor_from_probe, video_url_from_info
from .track_download import (
    TrackDownloadError,
    download_track_asset,
    relative_staging_path,
    source_failure_url,
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

    def claim_next_job(self) -> dict | None:
        for job_type in ("yt-dlp-asset", "yt-dlp"):
            job = self.client.claim_job(job_type, self.config.job_lease_secs)
            if job:
                return job
        return None

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
            job = self.claim_next_job()
            if not job:
                time.sleep(self.config.job_poll_secs)
                continue
            self._job_active = True
            try:
                logger.info("yt-dlp version for job: %s", ytdlp_version())
                if job.get("type") == "yt-dlp-asset":
                    self.process_asset_job(job)
                else:
                    self.process_job(job)
            except Exception as err:
                logger.exception("job %s failed", job.get("id"))
                if job.get("type") == "yt-dlp-asset":
                    self.submit_asset_failure(job, str(err))
                else:
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

    def process_asset_job(self, job: dict) -> None:
        job_id = job["id"]
        payload = json.loads(job["input"])
        entity_id = payload["entity_id"]
        asset_id = payload["asset_id"]

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
            entity = self.client.get_entity(entity_id)
            assets = entity.get("assets") or []
            selected_asset: dict | None = None
            asset = next((item for item in assets if item.get("id") == asset_id), None)
            selected_asset = asset
            if asset is None:
                raise TrackDownloadError(f"asset {asset_id} not found on entity {entity_id}")

            file_path = download_track_asset(
                asset,
                files_dir,
                extra_args=cookies_args(self.config),
            )
            rel_path = relative_staging_path(files_dir, file_path)
            result = self.client.commit_asset(entity_id, asset_id, job_id, rel_path)
            logger.info("asset job %s finished: %s", job_id, result)
        except TrackDownloadError as err:
            self.record_asset_failure(job, selected_asset, str(err))
            raise
        finally:
            stop.set()
            heartbeat_thread.join(timeout=2)

    def record_asset_failure(
        self,
        job: dict,
        asset: dict | None,
        message: str,
    ) -> None:
        external_id = "asset"
        url = None
        kind = "subtitle"
        source = "youtube"
        if asset:
            external_id = str(asset.get("id") or external_id)
            kind = str(asset.get("kind") or kind)
            url = source_failure_url(asset)
            metadata = asset.get("metadata") or {}
            if isinstance(metadata, dict):
                track_key = metadata.get("track_key")
                if isinstance(track_key, str) and ":" in track_key:
                    source = track_key.split(":", 1)[0]
        error_kind, _ = classify_error(message)
        self.record_failure(
            job_id=job.get("id"),
            kind=kind,
            external_id=external_id,
            url=url,
            stage="download",
            message=message,
            source=source if source not in {"subtitle", "audio"} else "youtube",
        )

    def submit_asset_failure(self, job: dict, message: str) -> None:
        job_id = job["id"]
        try:
            self.client.finish_job(job_id, status="failed")
        except Exception:
            logger.exception("failed to mark asset job %s failed", job_id)

    def process_job(self, job: dict) -> None:
        job_id = job["id"]
        parsed = parse_job_input(job["input"])
        input_url = parsed.url
        policy = parsed.asset_policy
        ytdlp_extra = cookies_args(self.config)
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
                extra_args=ytdlp_extra,
            )
            source = extractor_from_probe(probe)
            video_ids = require_entries(probe)
            video_states = self.client.source_states(video_ids, source=source, kind="video")
            missing = []
            if should_download_video(policy):
                missing = [
                    video_id
                    for video_id in video_ids
                    if should_download(video_states.get(video_id))
                ]
            thumb_ids = [thumbnail_external_id(video_id) for video_id in video_ids]
            thumbs_present = self.client.sources_has(thumb_ids, source=source, kind="thumbnail")
            missing_thumbs = []
            if should_download_thumbnail(policy):
                missing_thumbs = [
                    video_id
                    for video_id in video_ids
                    if video_states.get(video_id, {}).get("known")
                    and video_states.get(video_id, {}).get("has_present_asset")
                    and not thumbs_present.get(thumbnail_external_id(video_id))
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
                    source=source,
                    policy=policy,
                    extra_args=ytdlp_extra,
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
                    source=source,
                    extra_args=ytdlp_extra,
                    policy=policy,
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
                source=source,
                asset_policy=policy,
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
        source: str,
        policy,
        extra_args: list[str],
    ) -> tuple[list[dict], dict | None, list[dict]]:
        url = f"https://www.youtube.com/watch?v={video_id}" if source == "youtube" else None
        relations: list[dict] = []
        try:
            file_path, info, error = download_video(
                url or f"https://www.youtube.com/watch?v={video_id}",
                video_id,
                files_dir,
                format_selector=video_format_selector(policy),
                extra_args=extra_args,
            )
            if error:
                error_kind, _ = classify_error(error)
                item_status = error_kind_to_item_status(error_kind)
                self.record_failure(
                    job_id=job_id,
                    kind="video",
                    external_id=video_id,
                    url=url,
                    stage="download",
                    message=error,
                    source=source,
                )
                return (
                    [
                        build_item(
                            video_id=video_id,
                            relative_path=f"files/{video_id}.mp4",
                            status=item_status,
                            info=info,
                            source=source,
                            asset_policy=policy,
                        )
                    ],
                    channel_from_info(info) if info else None,
                    relations,
                )

            if file_path is None:
                assert info is not None
                url = video_url_from_info(info, video_id) or url
                return (
                    [
                        build_item(
                            video_id=video_id,
                            relative_path="",
                            status="discovered",
                            info=info,
                            source=source,
                            asset_policy=policy,
                        )
                    ],
                    channel_from_info(info),
                    relations,
                )

            assert info is not None
            url = video_url_from_info(info, video_id) or url
            _, file_meta = validate_download(file_path=file_path, info=info)
            file_meta["ytdlp_version"] = ytdlp_version()
            channel = channel_from_info(info)
            if channel:
                relations.append(
                    uploaded_by_relation(video_id, channel["external_id"], channel["source"])
                )

            items = [
                build_item(
                    video_id=video_id,
                    relative_path=relative_staging_path(files_dir, file_path),
                    status="complete",
                    info=with_thumbnail_metadata(info, video_id),
                    file_meta=file_meta,
                    source=source,
                    asset_policy=policy,
                )
            ]

            thumb_item = self.try_thumbnail(
                job_id=job_id,
                video_id=video_id,
                info=info,
                files_dir=files_dir,
                source=source,
                policy=policy,
            )
            if thumb_item:
                items.append(thumb_item)
                relations.append(thumbnail_relation(video_id, source))

            return items, channel, relations
        except (DownloadError, ValidationError, YtdlpError) as err:
            stage = "validate" if isinstance(err, ValidationError) else "download"
            message = str(err)
            error_kind, _ = classify_error(message)
            item_status = error_kind_to_item_status(error_kind)
            self.record_failure(
                job_id=job_id,
                kind="video",
                external_id=video_id,
                url=url,
                stage=stage,
                message=message,
                source=source,
            )
            return (
                [
                    build_item(
                        video_id=video_id,
                        relative_path=f"files/{video_id}.mp4",
                        status=item_status,
                        info=None,
                        source=source,
                        asset_policy=policy,
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
        source: str,
        extra_args: list[str],
        policy,
    ) -> tuple[list[dict], list[dict]]:
        relations: list[dict] = []
        if not should_download_thumbnail(policy):
            return [], relations
        page_url = (
            f"https://www.youtube.com/watch?v={video_id}"
            if source == "youtube"
            else f"https://unknown/{video_id}"
        )
        try:
            info = probe_video(page_url, extra_args=extra_args)
        except DownloadError as err:
            self.record_failure(
                job_id=job_id,
                kind="thumbnail",
                external_id=thumbnail_external_id(video_id),
                url=video_url_from_info({"id": video_id}, video_id),
                stage="download",
                message=str(err),
                source=source,
            )
            return [], relations

        thumb_item = self.try_thumbnail(
            job_id=job_id,
            video_id=video_id,
            info=info,
            files_dir=files_dir,
            source=source,
            policy=policy,
        )
        if thumb_item:
            relations.append(thumbnail_relation(video_id, source))
            return [thumb_item], relations
        return [], relations

    def try_thumbnail(
        self,
        *,
        job_id: str,
        video_id: str,
        info: dict,
        files_dir: Path,
        source: str,
        policy,
    ) -> dict | None:
        if not should_download_thumbnail(policy):
            return None
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
                source=source,
            )
            return None

        return build_thumbnail_item(
            video_id=video_id,
            relative_path=relative_staging_path(files_dir, thumb_path),
            info=info,
            source_thumbnail_url=source_url or "",
            source=source,
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
        source: str = "youtube",
    ) -> None:
        error_kind, retryable = classify_error(message)
        try:
            self.client.record_failure(
                job_id=job_id,
                source=source,
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
        failed = sum(
            1
            for item in video_items
            if item.get("status") not in {"complete", "discovered"}
        )
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


def should_download(state: dict | None) -> bool:
    if not state:
        return True
    if state.get("entity_status") == "user_deleted":
        return False
    if state.get("source_status") in {"dead", "private", "region_locked", "unavailable"}:
        return False
    return not bool(state.get("has_present_asset"))


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
