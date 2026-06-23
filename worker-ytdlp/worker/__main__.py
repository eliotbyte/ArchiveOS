from __future__ import annotations

import json
import logging
import shutil
import threading
import time
from pathlib import Path

import requests

from .channels import channel_from_info, uploaded_by_relation
from .config import Config
from .core_client import CoreClient, JobCancelled
from .discovery import discover, require_entries
from .download import DownloadError, download_video, probe_video
from .asset_policy import (
    is_metadata_only_refresh,
    should_download_channel_avatar,
    should_download_thumbnail,
    should_download_video,
    video_format_selector,
)
from .channel_avatars import resolve_author_probe_url, try_channel_avatar
from .job_input import parse_job_input
from .staging import (
    channel_avatar_in_staging,
    flush_staging,
    manifest_staging_paths,
    sanitize_manifest_for_staging,
    wait_for_staging_files,
)
from .failures import classify_error, classify_import_error, error_kind_to_item_status
from .job_status import LIFECYCLE_STATUSES, resolve_status_from_manifest, should_download
from .manifest_builder import (
    build_discovered_item,
    build_item,
    build_manifest,
    build_playlist_chunk_manifest,
    build_thumbnail_item,
    channel_from_probe,
    is_playlist,
    list_entries,
    relative_staging_path,
    thumbnail_relation,
    with_thumbnail_metadata,
)
from .source_mapper import extractor_from_probe, video_url_from_info
from .ytdlp_args import ytdlp_extra_args
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
from .progress import (
    ProgressReporter,
    build_asset_steps,
    build_progress,
    build_video_steps,
    video_labels_from_probe,
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
            except JobCancelled:
                logger.info("job %s cancelled", job.get("id"))
            except requests.HTTPError as err:
                logger.exception("job %s import failed", job.get("id"))
                if job.get("type") == "yt-dlp-asset":
                    self.submit_asset_failure(job, str(err))
                else:
                    self.handle_import_failure(job, str(err))
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
                    self.finish_job_failed(job)
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
        cancelled = threading.Event()

        def send_heartbeat() -> None:
            if self.client.heartbeat(job_id, self.config.job_lease_secs):
                cancelled.set()

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

        def check_cancelled() -> None:
            if cancelled.is_set():
                raise JobCancelled()

        try:
            check_cancelled()
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
                extra_args=ytdlp_extra_args(self.config),
            )
            rel_path = relative_staging_path(files_dir, file_path)
            result = self.client.commit_asset(entity_id, asset_id, job_id, rel_path)
            logger.info("asset job %s finished: %s", job_id, result)
        except TrackDownloadError as err:
            self.record_asset_failure(job, selected_asset, str(err))
            raise
        except JobCancelled:
            self.client.finish_job(job_id, status="cancelled")
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
        ytdlp_extra = ytdlp_extra_args(self.config)
        staging_dir = Path(self.config.vault_path) / "staging" / job_id
        files_dir = staging_dir / "files"
        if staging_dir.exists():
            shutil.rmtree(staging_dir)
        files_dir.mkdir(parents=True, exist_ok=True)

        stop = threading.Event()
        cancelled = threading.Event()

        def send_progress(progress: dict | None) -> None:
            if self.client.heartbeat(
                job_id,
                self.config.job_lease_secs,
                progress=progress,
            ):
                cancelled.set()

        progress_reporter = ProgressReporter(send_progress)

        def send_heartbeat() -> None:
            progress_reporter.tick()

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

        def check_cancelled() -> None:
            if cancelled.is_set():
                raise JobCancelled()

        try:
            check_cancelled()
            progress_reporter.update(
                build_progress(phase="discovering", label=input_url),
                force=True,
            )
            probe = discover(
                input_url,
                playlist_max_items=self.config.ytdlp_playlist_max_items,
                extra_args=ytdlp_extra,
            )
            source = extractor_from_probe(probe)
            if is_playlist(probe):
                self.process_playlist_job(
                    job_id=job_id,
                    input_url=input_url,
                    probe=probe,
                    parsed=parsed,
                    policy=policy,
                    ytdlp_extra=ytdlp_extra,
                    staging_dir=staging_dir,
                    files_dir=files_dir,
                    progress_reporter=progress_reporter,
                    check_cancelled=check_cancelled,
                )
                return

            video_ids = require_entries(probe)
            labels = video_labels_from_probe(probe)
            playlist_label = probe.get("title") or input_url
            video_states = self.client.source_states(video_ids, source=source, kind="video")
            thumb_ids = [thumbnail_external_id(video_id) for video_id in video_ids]
            thumb_states = self.client.source_states(thumb_ids, source=source, kind="thumbnail")
            missing = []
            if should_download_video(policy):
                missing = [
                    video_id
                    for video_id in video_ids
                    if should_download(video_states.get(video_id), resync=parsed.resync)
                ]
            missing_thumbs = []
            if should_download_thumbnail(policy):
                if is_metadata_only_refresh(policy):
                    missing_thumbs = list(video_ids)
                else:
                    missing_thumbs = [
                        video_id
                        for video_id in video_ids
                        if not thumb_states.get(
                            thumbnail_external_id(video_id), {}
                        ).get("has_present_asset")
                    ]

            channels: dict[str, dict] = {}
            relations: list[dict] = []
            avatar_download_failures = 0
            if is_metadata_only_refresh(policy):
                channel = channel_from_probe(probe, source)
                if channel:
                    channel = self.enrich_channel(
                        job_id=job_id,
                        channel=channel,
                        info=probe,
                        files_dir=files_dir,
                        policy=policy,
                        extra_args=ytdlp_extra,
                    )
                    channels[channel["external_id"]] = channel
                    if should_download_channel_avatar(policy) and not channel.get("avatar"):
                        avatar_download_failures += 1
                    for video_id in video_ids:
                        relations.append(
                            uploaded_by_relation(
                                video_id,
                                channel["external_id"],
                                source,
                            )
                        )

            logger.info(
                "job %s entries=%d missing_videos=%d missing_thumbs=%d",
                job_id,
                len(video_ids),
                len(missing),
                len(missing_thumbs),
            )

            items: list[dict] = []
            done_ids: set[str] = set()
            failed_ids: set[str] = set()
            thumb_download_failures = 0
            thumb_done_ids: set[str] = set()
            thumb_failed_ids: set[str] = set()

            for index, video_id in enumerate(missing, start=1):
                check_cancelled()
                logger.info("job %s acquiring %d/%d %s", job_id, index, len(missing), video_id)
                label = labels.get(video_id, video_id)
                progress_reporter.update(
                    build_progress(
                        phase="downloading",
                        current=index,
                        total=len(missing) or None,
                        label=label,
                        steps=build_video_steps(
                            missing,
                            labels,
                            running_id=video_id,
                            done_ids=done_ids,
                            failed_ids=failed_ids,
                        ),
                    ),
                    force=True,
                )

                def on_download_percent(percent: float, *, vid: str = video_id) -> None:
                    check_cancelled()
                    progress_reporter.update(
                        build_progress(
                            phase="downloading",
                            current=index,
                            total=len(missing) or None,
                            label=labels.get(vid, vid),
                            percent=percent,
                            steps=build_video_steps(
                                missing,
                                labels,
                                running_id=vid,
                                running_percent=percent,
                                done_ids=done_ids,
                                failed_ids=failed_ids,
                            ),
                        )
                    )

                acquired_items, channel, acquired_relations = self.acquire_video(
                    job_id=job_id,
                    video_id=video_id,
                    files_dir=files_dir,
                    source=source,
                    policy=policy,
                    extra_args=ytdlp_extra,
                    on_progress=on_download_percent,
                )
                item_status = (
                    acquired_items[0].get("status") if acquired_items else "failed"
                )
                if item_status == "complete":
                    done_ids.add(video_id)
                elif item_status not in {"discovered"}:
                    failed_ids.add(video_id)
                items.extend(acquired_items)
                if channel:
                    existing = channels.get(channel["external_id"])
                    if existing:
                        if (
                            existing.get("avatar")
                            and not channel.get("avatar")
                            and channel_avatar_in_staging(existing, staging_dir)
                        ):
                            channel = {**channel, "avatar": existing["avatar"]}
                        else:
                            channel = {**existing, **channel}
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
                thumb_id = thumbnail_external_id(video_id)
                progress_reporter.update(
                    build_progress(
                        phase="thumbnails",
                        current=index,
                        total=len(missing_thumbs) or None,
                        label=labels.get(video_id, video_id),
                        steps=build_asset_steps(
                            [thumbnail_external_id(vid) for vid in missing_thumbs],
                            {
                                thumbnail_external_id(vid): labels.get(vid, vid)
                                for vid in missing_thumbs
                            },
                            kind="thumbnail",
                            running_id=thumb_id,
                            done_ids=thumb_done_ids,
                            failed_ids=thumb_failed_ids,
                        ),
                    ),
                    force=True,
                )
                thumb_items, acquired_relations = self.acquire_thumbnail_only(
                    job_id=job_id,
                    video_id=video_id,
                    files_dir=files_dir,
                    source=source,
                    extra_args=ytdlp_extra,
                    policy=policy,
                )
                if not thumb_items:
                    thumb_download_failures += 1
                    thumb_failed_ids.add(thumb_id)
                else:
                    thumb_done_ids.add(thumb_id)
                items.extend(thumb_items)
                relations.extend(acquired_relations)

            progress_reporter.update(
                build_progress(phase="importing", label=playlist_label),
                force=True,
            )
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
            flush_staging(staging_dir)
            still_missing = wait_for_staging_files(
                staging_dir,
                manifest_staging_paths(manifest),
            )
            if still_missing:
                logger.warning(
                    "job %s staging still missing %d file(s) after wait: %s",
                    job_id,
                    len(still_missing),
                    ", ".join(still_missing[:5]),
                )
            manifest = sanitize_manifest_for_staging(manifest, staging_dir)
            status = resolve_status_from_manifest(manifest)
            if thumb_download_failures > 0 and not any(
                item.get("status") == "complete"
                and (item.get("source_ref") or {}).get("kind") == "thumbnail"
                for item in items
            ):
                status = "failed" if status == "done" else status
            elif thumb_download_failures > 0:
                status = "partial"
            if avatar_download_failures > 0 and status == "done":
                status = "partial"
            self._last_items = items
            self._last_resolved_status = status
            result = self.client.submit_manifest(
                job_id,
                manifest,
                status=status,
                max_attempts=2,
                retry_backoff_secs=1.0,
            )
            logger.info("job %s finished: %s", job_id, result)
        except JobCancelled:
            if staging_dir.exists():
                shutil.rmtree(staging_dir, ignore_errors=True)
            self.client.finish_job(job_id, status="cancelled")
            raise
        finally:
            stop.set()
            heartbeat_thread.join(timeout=2)

    def process_playlist_job(
        self,
        *,
        job_id: str,
        input_url: str,
        probe: dict,
        parsed,
        policy,
        ytdlp_extra: list[str],
        staging_dir: Path,
        files_dir: Path,
        progress_reporter: ProgressReporter,
        check_cancelled,
    ) -> None:
        source = extractor_from_probe(probe)
        entries = list_entries(probe)
        video_ids = [entry["id"] for entry in entries]
        labels = video_labels_from_probe(probe)
        playlist_label = probe.get("title") or input_url
        resync = parsed.resync
        video_states = self.client.source_states(video_ids, source=source, kind="video")
        thumb_ids = [thumbnail_external_id(video_id) for video_id in video_ids]
        thumb_states = self.client.source_states(thumb_ids, source=source, kind="thumbnail")

        channels: dict[str, dict] = {}
        relations: list[dict] = []
        items: list[dict] = []
        done_ids: set[str] = set()
        failed_ids: set[str] = set()

        logger.info(
            "job %s playlist incremental entries=%d resync=%s",
            job_id,
            len(entries),
            resync,
        )

        for position, entry in enumerate(entries):
            check_cancelled()
            video_id = entry["id"]
            label = labels.get(video_id, video_id)
            membership_entry = {
                "external_id": video_id,
                "position": position,
                "kind": "video",
                "url": entry.get("webpage_url") or entry.get("url"),
            }
            chunk_items: list[dict] = []
            chunk_relations: list[dict] = []

            needs_video = should_download_video(policy) and should_download(
                video_states.get(video_id),
                resync=resync,
            )
            thumb_id = thumbnail_external_id(video_id)
            needs_thumb = False
            if should_download_thumbnail(policy):
                if is_metadata_only_refresh(policy):
                    needs_thumb = True
                else:
                    needs_thumb = not thumb_states.get(thumb_id, {}).get(
                        "has_present_asset"
                    )

            if needs_video:
                download_index = len(done_ids) + len(failed_ids) + 1
                progress_reporter.update(
                    build_progress(
                        phase="downloading",
                        current=download_index,
                        total=len(video_ids),
                        label=label,
                        steps=build_video_steps(
                            video_ids,
                            labels,
                            running_id=video_id,
                            done_ids=done_ids,
                            failed_ids=failed_ids,
                        ),
                    ),
                    force=True,
                )

                def on_download_percent(percent: float, *, vid: str = video_id) -> None:
                    check_cancelled()
                    progress_reporter.update(
                        build_progress(
                            phase="downloading",
                            current=download_index,
                            total=len(video_ids),
                            label=labels.get(vid, vid),
                            percent=percent,
                            steps=build_video_steps(
                                video_ids,
                                labels,
                                running_id=vid,
                                running_percent=percent,
                                done_ids=done_ids,
                                failed_ids=failed_ids,
                            ),
                        )
                    )

                acquired_items, channel, acquired_relations = self.acquire_video(
                    job_id=job_id,
                    video_id=video_id,
                    files_dir=files_dir,
                    source=source,
                    policy=policy,
                    extra_args=ytdlp_extra,
                    on_progress=on_download_percent,
                )
                chunk_items.extend(acquired_items)
                item_status = (
                    acquired_items[0].get("status") if acquired_items else "failed"
                )
                if item_status == "complete":
                    done_ids.add(video_id)
                elif item_status not in {"discovered", *LIFECYCLE_STATUSES}:
                    failed_ids.add(video_id)
                if channel:
                    existing = channels.get(channel["external_id"])
                    if existing:
                        if (
                            existing.get("avatar")
                            and not channel.get("avatar")
                            and channel_avatar_in_staging(existing, staging_dir)
                        ):
                            channel = {**channel, "avatar": existing["avatar"]}
                        else:
                            channel = {**existing, **channel}
                    channels[channel["external_id"]] = channel
                chunk_relations.extend(acquired_relations)
            elif needs_thumb:
                thumb_items, acquired_relations = self.acquire_thumbnail_only(
                    job_id=job_id,
                    video_id=video_id,
                    files_dir=files_dir,
                    source=source,
                    extra_args=ytdlp_extra,
                    policy=policy,
                )
                if thumb_items:
                    chunk_items.extend(thumb_items)
                    done_ids.add(video_id)
                else:
                    chunk_items.append(
                        build_discovered_item(entry, source, policy)
                    )
                chunk_relations.extend(acquired_relations)
            else:
                chunk_items.append(build_discovered_item(entry, source, policy))

            items.extend(chunk_items)
            relations.extend(chunk_relations)

            progress_reporter.update(
                build_progress(
                    phase="importing",
                    current=position + 1,
                    total=len(entries),
                    label=label,
                ),
                force=True,
            )
            chunk_manifest = build_playlist_chunk_manifest(
                vault_name=self.config.vault_name,
                input_url=input_url,
                probe=probe,
                source=source,
                items=chunk_items,
                channels=list(channels.values()),
                relations=chunk_relations,
                membership=[membership_entry],
                asset_policy=policy,
            )
            flush_staging(staging_dir)
            still_missing = wait_for_staging_files(
                staging_dir,
                manifest_staging_paths(chunk_manifest),
            )
            if still_missing:
                logger.warning(
                    "job %s chunk staging still missing %d file(s): %s",
                    job_id,
                    len(still_missing),
                    ", ".join(still_missing[:5]),
                )
            chunk_manifest = sanitize_manifest_for_staging(chunk_manifest, staging_dir)
            self.client.import_chunk(
                job_id,
                chunk_manifest,
                finalize=False,
                max_attempts=2,
                retry_backoff_secs=1.0,
            )

        progress_reporter.update(
            build_progress(phase="importing", label=playlist_label),
            force=True,
        )
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
        status = resolve_status_from_manifest(manifest)
        self._last_items = items
        self._last_resolved_status = status
        result = self.client.import_chunk(
            job_id,
            manifest,
            finalize=True,
            status=status,
            max_attempts=2,
            retry_backoff_secs=1.0,
        )
        logger.info("job %s playlist finished: %s", job_id, result)

    def acquire_video(
        self,
        *,
        job_id: str,
        video_id: str,
        files_dir: Path,
        source: str,
        policy,
        extra_args: list[str],
        on_progress=None,
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
                on_progress=on_progress,
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
                channel = channel_from_info(info) if info else None
                if channel and info:
                    channel = self.enrich_channel(
                        job_id=job_id,
                        channel=channel,
                        info=info,
                        files_dir=files_dir,
                        policy=policy,
                        extra_args=extra_args,
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
                    channel,
                    relations,
                )

            if file_path is None:
                assert info is not None
                url = video_url_from_info(info, video_id) or url
                channel = channel_from_info(info)
                if channel:
                    channel = self.enrich_channel(
                        job_id=job_id,
                        channel=channel,
                        info=info,
                        files_dir=files_dir,
                        policy=policy,
                        extra_args=extra_args,
                    )
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
                    channel,
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
                channel = self.enrich_channel(
                    job_id=job_id,
                    channel=channel,
                    info=info,
                    files_dir=files_dir,
                    policy=policy,
                    extra_args=extra_args,
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

    def enrich_channel(
        self,
        *,
        job_id: str,
        channel: dict,
        info: dict | None,
        files_dir: Path,
        policy,
        extra_args: list[str],
    ) -> dict:
        if not should_download_channel_avatar(policy) or channel.get("avatar"):
            if channel.get("avatar") and not channel_avatar_in_staging(
                channel, files_dir.parent
            ):
                channel = {key: value for key, value in channel.items() if key != "avatar"}
            return channel
        author_url = channel.get("url") or resolve_author_probe_url(info or {})
        if not author_url:
            return channel
        avatar = try_channel_avatar(
            channel,
            info or {},
            files_dir,
            extra_args=extra_args,
        )
        if avatar:
            return {**channel, "avatar": avatar}
        self.record_failure(
            job_id=job_id,
            kind="avatar",
            external_id=channel["external_id"],
            url=author_url,
            stage="download",
            message="channel avatar unavailable",
            source=channel.get("source", "youtube"),
        )
        return channel

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
        error_kind: str | None = None,
        retryable: bool | None = None,
    ) -> None:
        if error_kind is None or retryable is None:
            classified_kind, classified_retryable = classify_error(message)
            if error_kind is None:
                error_kind = classified_kind
            if retryable is None:
                retryable = classified_retryable
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

    def handle_import_failure(self, job: dict, message: str) -> None:
        job_id = job.get("id")
        parsed = parse_job_input(job["input"])
        kind = "playlist" if "list=" in parsed.url else "video"
        error_kind, retryable = classify_import_error(message)
        self.record_failure(
            job_id=job_id,
            kind=kind,
            external_id=self._job_external_id(parsed.url),
            url=parsed.url,
            stage="import",
            message=message,
            error_kind=error_kind,
            retryable=retryable,
        )
        final_status = self._import_failure_status()
        self.finish_job_failed(job, status=final_status)

    def _import_failure_status(self) -> str:
        items = getattr(self, "_last_items", None) or []
        resolved = getattr(self, "_last_resolved_status", None)
        if resolved in {"partial", "failed"}:
            return resolved
        complete = sum(
            1 for item in items if item.get("status") == "complete"
        )
        if complete > 0:
            return "partial"
        return "failed"

    @staticmethod
    def _job_external_id(url: str) -> str:
        if "list=" in url:
            return url.split("list=", 1)[1].split("&", 1)[0]
        if "v=" in url:
            return url.split("v=", 1)[1].split("&", 1)[0]
        return "job"

    def finish_job_failed(self, job: dict, *, status: str = "failed") -> None:
        job_id = job["id"]
        try:
            self.client.finish_job(job_id, status=status)
        except Exception:
            logger.exception("failed to mark job %s %s", job_id, status)

    def submit_failure(self, job: dict) -> None:
        """Legacy path for unexpected failures before manifest is built."""
        self.finish_job_failed(job)


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
