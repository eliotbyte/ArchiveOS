# Video Archive MVP

ArchiveOS can catalog and download videos through yt-dlp, but practical daily use requires explicit archive intent: one-shot vs subscription, asset policy, cookies, job visibility, and preview semantics for future Web View.

## Scope

In scope:

- Structured yt-dlp job input with `mode`, `asset_policy`, and playlist resync options
- One-shot archive API
- Subscription archive API with stored policy
- Worker policy handling for video, thumbnail, subtitle, and audio catalog/download behavior
- yt-dlp cookies configuration
- Job list/detail/retry API
- Entity preview read-model for Web View readiness

Out of scope (deferred):

- Full Web View UI
- Web streaming and stream cache
- On-demand transcoding
- General derivatives subsystem and cache eviction
- Batch task groups
- External app launchers (mpv, RetroArch, etc.)

## Job Input

Legacy jobs store a plain URL string. New jobs store JSON:

```json
{
  "url": "https://youtube.com/playlist?list=...",
  "mode": "once",
  "resync": true,
  "removed_items": "mark_removed",
  "asset_policy": {
    "video": "best",
    "thumbnail": true,
    "subtitles": "preferred",
    "subtitle_languages": ["original", "en", "ru"],
    "automatic_subtitles": true,
    "audio_tracks": "main",
    "audio_languages": []
  }
}
```

- `mode=once`: archive now and stop
- `mode=subscription`: created by subscription scheduler on interval
- Legacy URL-only input normalizes to `mode=once` with default asset policy

## Asset Policy

| Field | Values | Default |
| --- | --- | --- |
| `video` | `best`, `best_1080p`, `audio_only`, `none` | `best` |
| `thumbnail` | boolean | `true` |
| `subtitles` | `none`, `manual`, `manual_auto`, `all`, `preferred` | `preferred` |
| `subtitle_languages` | list, `original` supported | `original`, `en`, `ru` |
| `automatic_subtitles` | boolean | `true` |
| `audio_tracks` | `none`, `main`, `preferred`, `all` | `main` |
| `audio_languages` | optional list | empty |

Worker behavior:

- `video=none`: discover/catalog only, no primary video download
- `thumbnail=false`: skip thumbnail download
- subtitle/audio catalog entries filtered by policy before manifest import
- remote track rows remain available for later `acquire` when cataloged

## Preview Semantics

Not all preview images are the same:

| Kind | Origin | Disposable? | MVP use |
| --- | --- | --- | --- |
| `source_thumbnail` | upstream poster/cover from yt-dlp | No | primary Web View card image |
| `generated_thumbnail` | frame extracted from local video | Yes (reproducible) | fallback when no source thumbnail |
| `timeline_preview` | storyboard/sprite from video | Yes | future Web View scrubbing |
| `preview_video` | short generated clip | Yes | future Web View hover preview |

YouTube videos usually already have source thumbnails. Web View should prefer them. Generated thumbnails are derivatives and must not replace or delete source artwork.

Entity detail exposes optional `preview`:

```json
{
  "entity_id": "...",
  "asset_id": "...",
  "kind": "thumbnail",
  "preview_role": "source_thumbnail",
  "status": "present"
}
```

## API

- `POST /vaults/{vault}/archive` — one-shot archive with structured body
- `POST /vaults/{vault}/subscribe` — recurring archive with stored policy
- `GET /vaults/{vault}/jobs` — list jobs (`status`, `type` filters)
- `GET /vaults/{vault}/jobs/{id}` — job detail
- `POST /vaults/{vault}/jobs/{id}/retry` — requeue failed/done job

Existing endpoints remain:

- `POST /vaults/{vault}/jobs` with string input (legacy)
- `GET /vaults/{vault}/source-failures`
- entity detail with assets and preview

## Worker Configuration

```env
YTDLP_COOKIES_PATH=/vaults/archiveos/config/cookies.txt
```

When set, worker passes `--cookies` to yt-dlp probes and downloads.

## Minimal Runbook

```powershell
$env:VAULT_HOST_PATH="A:/archiveos"
docker compose -f docker-compose.yml -f docker-compose.local.yml up -d
```

Create one-shot archive:

```http
POST /vaults/archiveos/archive
{
  "url": "https://www.youtube.com/watch?v=...",
  "asset_policy": { "video": "best", "thumbnail": true, "subtitles": "preferred" }
}
```

Create subscription:

```http
POST /vaults/archiveos/subscribe
{
  "source": "youtube",
  "kind": "playlist",
  "url": "https://www.youtube.com/playlist?list=...",
  "interval_minutes": 360,
  "resync": true,
  "asset_policy": { "video": "best", "subtitles": "preferred" }
}
```

Inspect jobs and retry failures via `/jobs` and `/jobs/{id}/retry`.
