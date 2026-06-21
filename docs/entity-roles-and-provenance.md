Entity roles and metadata provenance

Some entities are user-visible media; others are supporting assets (thumbnails, covers, subtitles, etc.) that should stay in the graph and CAS but not show up in normal galleries or search. We express that with metadata rows, not a separate entity_type column.

Primary entities have no visibility=hidden. Supporting assets carry entity_role=supporting, asset_role=<thumbnail|cover|subtitle|booklet|waveform>, and visibility=hidden. These role/visibility fields are written with provenance archiveos.

Each metadata row has a provenance label saying where the value came from:

user — manual edits and overrides
yt-dlp — source metadata from the downloader (title, description, channel, source_thumbnail_url, …)
ffprobe — values taken from the actual file after inspection (codec, height, duration, validation_status, …)
archiveos — system-generated links and roles (thumbnail_external_id, entity_role, asset_role, visibility, thumbnail_for)
inferred — filename or blob-derived guesses (width/height from image headers, etc.)
system — internal import/path bookkeeping
source, extracted — legacy labels kept for older rows; new imports should use the labels above

The same key may exist under multiple provenances. API consumers get a flat metadata map where one value wins per key. Precedence: user > yt-dlp > ffprobe > archiveos > inferred > system > source > extracted. Entity detail also returns metadata_entries with every row and its provenance.

Import manifests may send metadata_by_provenance: a map from provenance label to a JSON object of fields. Legacy manifests with only metadata still work: yt-dlp worker manifests store that block as yt-dlp; other sources keep source until migrated.

YouTube thumbnails are normal image/* entities with source_ref kind=thumbnail and external_id {video_id}:thumbnail. The worker puts yt-dlp fields (title, source_thumbnail_url) under yt-dlp and role/visibility under archiveos. Core links video → thumbnail via entity_relation and sets thumbnail_external_id on the video (archiveos).

Search excludes entities with visibility=hidden by default. Pass include_hidden=true on GET /vaults/{vault}/search to include them (debug, admin, or explicit “show attachments” later).

Migration 004 backfills existing thumbnail entities with supporting/hidden role metadata and thumbnail_external_id on parent videos where a thumbnail relation already exists. It does not rewrite every historical source row.
