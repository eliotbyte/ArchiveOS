# Web View MVP

ArchiveOS now ships a minimal browser explorer for archived media. It is read-mostly with job triggers for archive, acquire, and retry.

## Scope

In scope:

- React explorer under `web/`
- Browse recent entities without search params
- Card grid with preview thumbnails and status badges
- Entity detail with assets, metadata, browser displayability notes
- Archive URL form, jobs list, retry failed jobs
- Acquire remote subtitle/audio assets from entity detail
- Asset content streaming via HTTP

Out of scope:

- Generated thumbnail worker fallback
- Browser streaming/transcode for unsupported video containers
- Collections browser UI
- Auth / multi-user access control

## API

New endpoints:

- `GET /vaults/{vault}/entities?limit=&query=&kind=&source=&status=&include_hidden=`
- `GET /vaults/{vault}/assets/{asset_id}/content`

Existing endpoints used by the UI:

- `GET /vaults/{vault}/entities/{id}`
- `POST /vaults/{vault}/archive`
- `GET /vaults/{vault}/jobs`
- `POST /vaults/{vault}/jobs/{id}/retry`
- `POST /vaults/{vault}/entities/{entity_id}/assets/{asset_id}/acquire`

## Worker directories

Vault-local worker data lives under:

```text
{vault}/workers/ytdlp/cookies/youtube.txt
{vault}/workers/ytdlp/cache/
{vault}/workers/thumbnail/cache/
```

If `YTDLP_COOKIES_PATH` is unset and `workers/ytdlp/cookies/youtube.txt` exists, the yt-dlp worker uses it automatically.

## Docker (recommended)

Web UI baked into `core` image. One port for API + explorer.

```powershell
$env:VAULT_HOST_PATH="A:/archiveos"
docker compose -f docker-compose.yml -f docker-compose.local.yml up -d --build
```

Open `http://localhost:8080`.

Rebuild after web changes:

```powershell
docker compose -f docker-compose.yml -f docker-compose.local.yml build core
docker compose -f docker-compose.yml -f docker-compose.local.yml up -d core
```

Optional cookies:

```powershell
New-Item -ItemType Directory -Force -Path "A:/archiveos/workers/ytdlp/cookies"
Copy-Item cookies.txt "A:/archiveos/workers/ytdlp/cookies/youtube.txt"
```

## Local dev (optional)

Use only when hacking `web/` without rebuilding Docker:

```powershell
cd web
npm install
$env:VITE_VAULT_NAME="archiveos"
npm run dev
```

Vite dev server proxies `/api` → `http://localhost:8080`. Pick any free port if `5173` is taken:

```powershell
npm run dev -- --port 5174
```

## UI behavior

- Cards load preview bytes from `/assets/{asset_id}/content`
- Missing preview → card shows `No preview`
- Non-present preview asset → card shows pending state
- Primary media only renders inline when browser MIME is supported
- Unsupported primary media still shows asset status and acquire actions

## Production note

Docker `core` serves built static files from `/usr/share/archiveos/web` and falls back to `index.html` for client routes.

Build args:

- `VITE_VAULT_NAME` — vault name baked into UI (default `default`, local override `archiveos`)
- `VITE_API_BASE` — empty for same-origin Docker; set only for external API host
