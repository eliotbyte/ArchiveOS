# Source Entities And Assets

ArchiveOS models meaning separately from bytes.

An `entity` is the thing the user cares about: a YouTube video, playlist, channel,
local image, album, or other logical object. A file on disk is not the entity. It
is an asset/rendition attached to an entity.

## Core Objects

- `entity`: logical object with lifecycle state. It may exist without local bytes.
- `entity_asset`: physical or referenced bytes attached to an entity. A video can
  have multiple assets, such as different qualities, thumbnails, subtitles, or
  reference paths.
- `source_ref`: external identity for an entity, such as
  `(youtube, video, dQw4...)` or `(youtube, playlist, PL...)`.
- `collection`: aggregate entity. YouTube playlists and channel uploads are
  collections with source identity.
- `collection_member`: source-observed membership between a collection and an
  entity. Removing a video from a playlist changes membership state, not the
  video entity or its local assets.

## Lifecycle Layers

Entity status answers: should this logical object be visible and managed?

- `active`: normal entity.
- `user_deleted`: user intentionally removed the logical object from ArchiveOS.
  Source identity remains as a tombstone so workers do not recreate it.
- `source_deleted`: source says the logical object no longer exists. Local assets
  are retained unless the user deletes them.

Asset status answers: do we have usable bytes?

- `present`: bytes/path are available.
- `missing_local`: ArchiveOS expected local bytes, but filesystem reconciliation
  could not find them.
- `deleted_by_user`: user intentionally deleted this asset through ArchiveOS.
- `download_failed`: acquisition failed before usable bytes were committed.
- `partial`: incomplete bytes exist but are not usable.
- `remote`: source-reported track (subtitle/audio) cataloged but not downloaded yet.

Source status answers: what does the upstream source say?

- `live`: source identity is currently valid.
- `dead`: upstream reports deleted or permanently unavailable.
- `unavailable`: source could not be checked or is temporarily blocked.
- `private` / `region_locked`: optional first-class unavailable states when the
  worker can distinguish them.

Membership status answers: is this entity still part of this source aggregate?

- `active`: source still lists the member.
- `removed_from_source`: source no longer lists it.
- `user_removed`: user hid/removed the member locally without deleting the entity.

## Deletion Matrix

| Scenario | Entity | Asset | Source ref | Membership |
| --- | --- | --- | --- | --- |
| User deletes logical video | `user_deleted` | `deleted_by_user` after blob removal | preserved | unchanged unless explicitly removed |
| User deletes only local file | unchanged | `deleted_by_user` | preserved | unchanged |
| File disappears outside ArchiveOS | unchanged | `missing_local` | preserved | unchanged |
| YouTube video is deleted/private | usually `active` | unchanged | `dead` or specific unavailable status | unchanged |
| Video removed from playlist | unchanged | unchanged | unchanged | `removed_from_source` |
| User removes video from playlist view | unchanged | unchanged | unchanged | `user_removed` |

Workers may redownload only when the entity is not `user_deleted`, the source is
not permanently dead, and no `present` asset satisfies the requested role/kind.

## yt-dlp Source Mapping

Import manifests from the yt-dlp worker use two source labels:

- `manifest.source = yt-dlp` — worker/provenance identity
- `manifest.source_identity = <extractor>` — platform identity for `source_ref` rows, e.g. `youtube`, `pornhub`, `vimeo`

Each supported extractor maps the same way:

- Video URL → `entity` with `source_ref(source=<extractor>, kind=video)`
- Downloaded file → `entity_asset` with `role=primary`, `kind=video`, `status=present`
- Thumbnail → hidden supporting entity (transition) plus `entity_asset` with `kind=thumbnail`
- Playlist / uploads feed → collection with `collection.type=<extractor>_playlist` or `<extractor>_channel_uploads`
- Channel / uploader → `source_ref(kind=channel)` only when a stable `channel_id` or `uploader_id` exists
- Subtitles and audio tracks → `entity_asset` rows on the video entity with `status=remote`, `storage_strategy=remote`, and asset metadata (`language`, `caption_kind`, `format_id`, `source_url`, `source_page_url`, …) in `entity_asset_metadata`

Remote track acquisition:

1. Entity detail exposes each asset with flattened `metadata` plus provenance-aware `metadata_entries`.
2. `POST /vaults/{vault}/entities/{entity_id}/assets/{asset_id}/acquire` creates a `yt-dlp-asset` job when the asset is `remote` or `missing_local`.
3. The yt-dlp worker claims `yt-dlp-asset` jobs, downloads the selected subtitle (HTTP GET on `source_url`) or audio track (`yt-dlp -f <format_id> <source_page_url>`), then commits bytes through `POST .../commit`.
4. Commit stores the staging file in CAS and updates the same `entity_asset` row to `status=present` without creating a new entity.

Failures record a source failure and mark the job failed; the remote catalog row stays available for retry.

The worker skips metadata fields that are absent or meaningless (`null`, empty title/description). Service-specific fields (Pornhub `categories`/`cast`, YouTube `availability`, etc.) stay under provenance `yt-dlp`. Localized title/description variants are stored as `localized_text` JSON under `yt-dlp` metadata when available.

Legacy manifests without `source_identity` still default membership/collection identity to `youtube` when `manifest.source = yt-dlp`.

## YouTube Example

- YouTube video URL creates or updates an `entity` with
  `source_ref(source=youtube, kind=video)`.
- The downloaded `.mp4` is an `entity_asset` with `role=primary`, `kind=video`.
- Additional qualities become additional video assets on the same entity.
- Thumbnail files are `entity_asset` rows with `role=supporting`,
  `kind=thumbnail`. During the transition they may still be wrapped by hidden
  supporting entities so existing thumbnail relations and debug search keep
  working.
- YouTube playlist URL creates a collection entity with
  `collection.type=youtube_playlist` and active members for every discovered
  video, even when some videos are not downloaded.
- YouTube channel/uploads URL creates a channel entity and a collection entity
  with `collection.type=youtube_channel_uploads`.

## API Notes

- `GET /vaults/{vault}/entities/{id}` returns legacy top-level `content_hash`,
  `mime`, and `size` plus canonical `assets[]`. Each asset includes `metadata`
  (flattened best-value map) and `metadata_entries` (provenance-aware rows from
  `entity_asset_metadata`).
- `POST /vaults/{vault}/entities/{entity_id}/assets/{asset_id}/acquire` creates
  a `yt-dlp-asset` job for remote/missing_local subtitle or audio assets.
- `POST /vaults/{vault}/entities/{entity_id}/assets/{asset_id}/commit` is
  worker-facing: `{ "job_id": "...", "path": "files/<name>" }` commits a file
  from `staging/{job_id}` into CAS and marks the asset `present`.
- `DELETE /vaults/{vault}/collections/{collection_id}/members/{entity_id}` sets
  membership status to `user_removed` without deleting the entity or assets.
- `POST /vaults/{vault}/reconcile/assets` marks present assets whose paths are
  missing as `missing_local`.
- Set `ARCHIVEOS_ASSET_RECONCILE_SECS` (and `ARCHIVEOS_DEFAULT_VAULT`) to run
  asset reconciliation periodically; disable with
  `ARCHIVEOS_ASSET_RECONCILE_SCHEDULER=off`.

See [video-archive-mvp.md](video-archive-mvp.md) for structured archive jobs,
asset policy, subscriptions, cookies, and preview semantics.
