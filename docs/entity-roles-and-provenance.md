Entity roles and metadata provenance

See also [source-entities-and-assets.md](source-entities-and-assets.md) for the logical entity vs asset model, lifecycle statuses, and deletion matrix.

Some entities are user-visible media; others are supporting assets (thumbnails, covers, subtitles, etc.) that should stay in the graph and CAS but not show up in normal galleries or search. We express that with metadata rows, not a separate entity_type column.

Primary entities have no visibility=hidden. Supporting assets carry entity_role=supporting, asset_role=<thumbnail|cover|subtitle|booklet|waveform>, and visibility=hidden. These role/visibility fields are written with provenance archiveos.

YouTube thumbnails are a compatibility wrapper during the entity/asset transition: import still creates a hidden supporting thumbnail entity (for entity_relation thumbnail, hidden search behavior, and thumbnail_external_id on the parent video), and also writes an entity_asset row with role=supporting and kind=thumbnail on that hidden entity. Canonical bytes for thumbnails live in entity_asset; the hidden entity keeps graph/search compatibility until UI/API can address assets directly.

Each metadata row has a provenance label saying where the value came from:

user — manual edits and overrides
yt-dlp — source metadata from the downloader (title, description, channel, source_thumbnail_url, …)
ffprobe — values taken from the actual file after inspection (codec, height, duration, validation_status, …)
archiveos — system-generated links and roles (thumbnail_external_id, entity_role, asset_role, visibility, thumbnail_for)
inferred — filename or blob-derived guesses (width/height from image headers, etc.)
system — internal import/path bookkeeping
source, extracted — legacy labels kept for older rows; new imports should use the labels above

The same key may exist under multiple provenances. API consumers get a flat metadata map where one value wins per key. Precedence: user > yt-dlp > ffprobe > archiveos > inferred > system > source > extracted. Entity detail also returns metadata_entries with every row and its provenance.

Import manifests may send:

- `metadata_by_provenance` on items and channels: map from provenance label to JSON fields
- `assets[]` on items: catalog entries for remote subtitle/audio tracks before download
- `source_identity` on the manifest: extractor/platform id separate from worker name `yt-dlp`

Legacy manifests with only `metadata` still work: yt-dlp worker manifests store that block as `yt-dlp`; other sources keep `source` until migrated.

Optional fields are omitted, not invented: if an extractor returns no description (`null`) or no stable channel/uploader id, the worker does not write description metadata or create a channel entity.

Subtitle/audio catalog assets use `entity_asset_metadata` for per-track fields (`language`, `caption_kind`, `format_id`, `source_url`, `source_page_url`, `track_key`). Entity detail returns these as per-asset `metadata` and `metadata_entries`. When bytes are later downloaded, the worker commits staging bytes through `POST .../assets/{asset_id}/commit` and the same asset row moves from `status=remote` to `status=present`.

Localized text uses repeatable records under `yt-dlp.localized_text`: `{field, language, value, is_primary, is_translated}`. YouTube translated title/description probing can add more entries via optional worker language config without blocking the primary download.

Thumbnails use source_ref kind=thumbnail and external_id `{video_id}:thumbnail` with the parent video's extractor as `source_ref.source`.

Search excludes entities with visibility=hidden by default. Pass include_hidden=true on GET /vaults/{vault}/search to include them (debug, admin, or explicit “show attachments” later).

Entity detail API: entity.content_hash, entity.mime, and entity.size are legacy read-model fields kept for older clients. Canonical bytes and renditions are in the assets array on the same response. Each asset includes flattened metadata plus full metadata_entries with provenance.

Migration 004 backfills existing thumbnail entities with supporting/hidden role metadata and thumbnail_external_id on parent videos where a thumbnail relation already exists. It does not rewrite every historical source row.
