export const API_BASE =
  import.meta.env.VITE_API_BASE ?? (import.meta.env.DEV ? "/api" : "");
export const DEFAULT_VAULT_NAME = import.meta.env.VITE_VAULT_NAME ?? "archiveos";

/** @deprecated use useVault() */
export const VAULT_NAME = DEFAULT_VAULT_NAME;

export class ApiError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, {
    headers: {
      Accept: "application/json",
      ...(init?.body ? { "Content-Type": "application/json" } : {}),
      ...init?.headers,
    },
    ...init,
  });

  if (!response.ok) {
    let message = response.statusText;
    try {
      const body = (await response.json()) as { error?: string };
      if (body.error) message = body.error;
    } catch {
      // ignore non-json errors
    }
    throw new ApiError(response.status, message);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return (await response.json()) as T;
}

export interface AssetPolicy {
  video: string;
  thumbnail: boolean;
  subtitles: string;
  subtitle_languages: string[];
  automatic_subtitles: boolean;
  audio_tracks: string;
  audio_languages?: string[];
}

export interface ArchiveRequest {
  url: string;
  mode?: "once" | "subscription";
  asset_policy?: Partial<AssetPolicy>;
}

export interface VaultRegistryEntry {
  name: string;
  path: string;
}

export interface WorkerCapability {
  id: string;
  label: string;
  enabled: boolean;
}

export interface VaultCapabilities {
  workers: WorkerCapability[];
}

export function createApi(vaultName: string) {
  return {
    listVaults: () => request<VaultRegistryEntry[]>("/vaults"),

    getCapabilities: () =>
      request<VaultCapabilities>(`/vaults/${vaultName}/capabilities`),

    listEntities: (
      params: Record<string, string | number | boolean | undefined> = {},
    ) => {
      const query = new URLSearchParams();
      for (const [key, value] of Object.entries(params)) {
        if (value !== undefined && value !== "") {
          query.set(key, String(value));
        }
      }
      const suffix = query.size > 0 ? `?${query.toString()}` : "";
      return request<EntityListItem[]>(`/vaults/${vaultName}/entities${suffix}`);
    },

    getEntity: (id: string) =>
      request<EntityDetail>(`/vaults/${vaultName}/entities/${id}`),

    searchEntities: (query: string) =>
      request<EntityHit[]>(
        `/vaults/${vaultName}/search?${new URLSearchParams({ query }).toString()}`,
      ),

    listCollections: (
      params: {
        type?: string;
        min_member_count?: number;
      } = {},
    ) => {
      const query = new URLSearchParams();
      if (params.type) query.set("type", params.type);
      if (params.min_member_count !== undefined) {
        query.set("min_member_count", String(params.min_member_count));
      }
      const suffix = query.size > 0 ? `?${query.toString()}` : "";
      return request<CollectionSummary[]>(
        `/vaults/${vaultName}/collections${suffix}`,
      );
    },

    getCollection: (collectionId: string) =>
      request<CollectionDetail>(
        `/vaults/${vaultName}/collections/${collectionId}`,
      ),

    listCollectionMembers: (collectionId: string) =>
      request<CollectionMemberItem[]>(
        `/vaults/${vaultName}/collections/${collectionId}/members`,
      ),

    listJobs: (params: {
      status?: string;
      type?: string;
      limit?: number;
    } = {}) => {
      const query = new URLSearchParams();
      if (params.status) query.set("status", params.status);
      if (params.type) query.set("type", params.type);
      query.set("limit", String(params.limit ?? 100));
      return request<Job[]>(`/vaults/${vaultName}/jobs?${query.toString()}`);
    },

    getJob: (jobId: string) =>
      request<Job>(`/vaults/${vaultName}/jobs/${jobId}`),

    retryJob: (jobId: string) =>
      request<Job>(`/vaults/${vaultName}/jobs/${jobId}/retry`, {
        method: "POST",
      }),

    listSubscriptions: () =>
      request<Subscription[]>(`/vaults/${vaultName}/subscriptions`),

    deleteSubscription: (id: string) =>
      request<{ removed: string }>(`/vaults/${vaultName}/subscriptions/${id}`, {
        method: "DELETE",
      }),

    listSourceFailures: (params: { source?: string; kind?: string } = {}) => {
      const query = new URLSearchParams();
      if (params.source) query.set("source", params.source);
      if (params.kind) query.set("kind", params.kind);
      const suffix = query.size > 0 ? `?${query.toString()}` : "";
      return request<SourceFailure[]>(
        `/vaults/${vaultName}/source-failures${suffix}`,
      );
    },

    archiveUrl: (body: ArchiveRequest) =>
      request<Job>(`/vaults/${vaultName}/archive`, {
        method: "POST",
        body: JSON.stringify({
          url: body.url,
          mode: body.mode ?? "once",
          asset_policy: {
            video: "best",
            thumbnail: true,
            subtitles: "preferred",
            subtitle_languages: ["original", "en", "ru"],
            automatic_subtitles: true,
            audio_tracks: "main",
            ...body.asset_policy,
          },
        }),
      }),

    subscribeUrl: (body: ArchiveRequest & { interval_minutes: number }) =>
      request<Subscription>(`/vaults/${vaultName}/subscribe`, {
        method: "POST",
        body: JSON.stringify({
          source: "youtube",
          kind: "playlist",
          url: body.url,
          interval_minutes: body.interval_minutes,
          asset_policy: {
            video: "best",
            thumbnail: true,
            subtitles: "preferred",
            subtitle_languages: ["original", "en", "ru"],
            automatic_subtitles: true,
            audio_tracks: "main",
            ...body.asset_policy,
          },
        }),
      }),

    acquireAsset: (entityId: string, assetId: string) =>
      request<Job>(
        `/vaults/${vaultName}/entities/${entityId}/assets/${assetId}/acquire`,
        { method: "POST" },
      ),

    regeneratePreviews: (entityId: string) =>
      request<Job>(`/vaults/${vaultName}/entities/${entityId}/previews/regenerate`, {
        method: "POST",
      }),

    backfillPreviews: () =>
      request<PreviewBackfillReport>(`/vaults/${vaultName}/previews/backfill`, {
        method: "POST",
      }),

    assetContentUrl: (assetId: string) =>
      `${API_BASE}/vaults/${vaultName}/assets/${assetId}/content`,
  };
}

/** Legacy singleton for tests and gradual migration */
export const api = createApi(DEFAULT_VAULT_NAME);

export function assetContentUrl(assetId: string): string {
  return api.assetContentUrl(assetId);
}

export interface EntityPreviewSummary {
  entity_id: string;
  asset_id: string;
  kind: string;
  preview_role: string;
  status: string;
}

export interface EntityListItem {
  id: string;
  title?: string | null;
  mime?: string | null;
  kind?: string | null;
  status: string;
  source?: string | null;
  tags: string[];
  preview?: EntityPreviewSummary | null;
  timeline_sprite?: EntityPreviewSummary | null;
  timeline_manifest?: EntityPreviewSummary | null;
  primary_asset_id?: string | null;
  primary_asset_status?: string | null;
  channel?: string | null;
  uploader?: string | null;
  duration?: string | null;
}

export interface CollectionSummary {
  id: string;
  collection_type: string;
  title: string;
  member_count: number;
  cover_preview?: EntityPreviewSummary | null;
}

export interface CollectionDetail {
  id: string;
  collection_type: string;
  title: string;
  member_count: number;
  cover_preview?: EntityPreviewSummary | null;
  members: CollectionMemberItem[];
}

export interface CollectionMemberItem {
  id: string;
  title?: string | null;
  kind?: string | null;
  mime?: string | null;
  source?: string | null;
  status: string;
  position: number;
  preview?: EntityPreviewSummary | null;
  timeline_sprite?: EntityPreviewSummary | null;
  timeline_manifest?: EntityPreviewSummary | null;
  primary_asset_id?: string | null;
  primary_asset_status?: string | null;
  duration?: string | null;
  channel?: string | null;
  uploader?: string | null;
  webpage_url?: string | null;
}

export interface EntityHit {
  id: string;
  title?: string | null;
  mime?: string | null;
  tags: string[];
}

export interface EntityAsset {
  id: string;
  role: string;
  kind: string;
  content_hash?: string | null;
  mime?: string | null;
  size: number;
  ext?: string | null;
  status: string;
  storage_strategy: string;
  path?: string | null;
  metadata: Record<string, string>;
}

export interface EntityDetail {
  id: string;
  title?: string;
  mime?: string | null;
  status: string;
  added_at: string;
  tags: string[];
  metadata: Record<string, string>;
  assets: EntityAsset[];
  preview?: EntityPreviewSummary | null;
  timeline_sprite?: EntityPreviewSummary | null;
  timeline_manifest?: EntityPreviewSummary | null;
}

export interface Job {
  id: string;
  type: string;
  status: string;
  input: string;
  attempts: number;
  created_at: string;
  lease_until?: string | null;
  target_vault?: string;
}

export interface Subscription {
  id: string;
  source: string;
  kind: string;
  url: string;
  target_vault: string;
  interval_minutes: number;
  next_run_at: string;
  last_checked_at?: string | null;
  status: string;
  created_at: string;
}

export interface PreviewBackfillReport {
  scanned: number;
  queued: number;
  skipped: number;
}

export interface SourceFailure {
  id: string;
  source: string;
  kind: string;
  external_id: string;
  url?: string | null;
  stage: string;
  error_kind: string;
  message: string;
  retryable: boolean;
  created_at: string;
}

export type BrowserDisplayState = "supported" | "unsupported" | "unknown";

const BROWSER_MIME_PREFIXES = ["image/", "video/", "audio/", "text/"];
const BROWSER_MIME_EXACT = new Set(["application/pdf"]);

export function browserDisplayState(mime?: string | null): BrowserDisplayState {
  if (!mime) return "unknown";
  if (BROWSER_MIME_EXACT.has(mime)) return "supported";
  if (BROWSER_MIME_PREFIXES.some((prefix) => mime.startsWith(prefix))) {
    return "supported";
  }
  return "unsupported";
}

export type PreviewVisualState =
  | "loading"
  | "ready"
  | "missing"
  | "pending"
  | "failed";

export function previewVisualState(
  preview?: EntityPreviewSummary | null,
  imageFailed = false,
): PreviewVisualState {
  if (!preview) return "missing";
  if (preview.status !== "present") return "pending";
  if (imageFailed) return "failed";
  return "ready";
}

export interface TimelineManifest {
  version: number;
  tile_width: number;
  tile_height: number;
  columns: number;
  rows: number;
  duration_secs: number;
  frames: Array<{ index: number; start_secs: number; end_secs: number }>;
}

export function kindIcon(kind?: string | null): string {
  switch (kind) {
    case "video":
      return "▶";
    case "image":
      return "◻";
    case "audio":
      return "♫";
    default:
      return "•";
  }
}

export type MediaKind = "video" | "image" | "audio" | "file" | "unknown";

export function mediaKindFromEntity(
  kind?: string | null,
  mime?: string | null,
): MediaKind {
  if (kind === "video" || kind === "image" || kind === "audio") {
    return kind;
  }
  if (kind === "file") {
    if (mime?.startsWith("video/")) return "video";
    if (mime?.startsWith("image/")) return "image";
    if (mime?.startsWith("audio/")) return "audio";
    return "file";
  }
  if (mime?.startsWith("video/")) return "video";
  if (mime?.startsWith("image/")) return "image";
  if (mime?.startsWith("audio/")) return "audio";
  if (mime) return "file";
  return "unknown";
}

export function displayTitle(
  title?: string | null,
  metadata?: Record<string, string>,
  id?: string,
): string {
  const fromMeta = metadata?.title ?? metadata?.name;
  if (fromMeta && fromMeta.trim()) return fromMeta;
  if (title && title.trim()) return title;
  return id?.slice(0, 8) ?? "Untitled";
}

export function isFilenameTitle(title: string): boolean {
  return /\.(jpe?g|png|gif|webp|mp4|mkv|mov|avi|webm|pdf)$/i.test(title);
}
