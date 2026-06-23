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

export interface UserMediaPreferences {
  subtitle_languages: string[];
  subtitle_mode: string;
  audio_languages: string[];
  audio_mode: string;
  video_quality: string;
  thumbnail: boolean;
  automatic_subtitles: boolean;
}

export function preferencesToAssetPolicy(
  prefs: UserMediaPreferences,
): AssetPolicy {
  const video =
    prefs.video_quality === "1080" || prefs.video_quality === "720"
      ? prefs.video_quality
      : prefs.video_quality === "best_1080p"
        ? "best_1080p"
        : "best";
  return {
    video,
    thumbnail: prefs.thumbnail,
    subtitles: prefs.subtitle_mode,
    subtitle_languages: prefs.subtitle_languages,
    automatic_subtitles: prefs.automatic_subtitles,
    audio_tracks: prefs.audio_mode,
    audio_languages: prefs.audio_languages,
  };
}

export const DEFAULT_MEDIA_PREFERENCES: UserMediaPreferences = {
  subtitle_languages: ["ru", "en", "original"],
  subtitle_mode: "preferred",
  audio_languages: ["original", "ru", "en"],
  audio_mode: "preferred",
  video_quality: "best",
  thumbnail: true,
  automatic_subtitles: true,
};

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

    deleteEntity: (id: string) =>
      request<{ entity_id: string; status: string }>(
        `/vaults/${vaultName}/entities/${id}`,
        { method: "DELETE" },
      ),

    removeCollectionMember: (collectionId: string, entityId: string) =>
      request<{
        collection_id: string;
        entity_id: string;
        status: string;
      }>(
        `/vaults/${vaultName}/collections/${collectionId}/members/${entityId}`,
        { method: "DELETE" },
      ),

    refreshEntityFromSource: (
      entityId: string,
      options: { metadata_only?: boolean } = {},
    ) =>
      request<Job>(
        `/vaults/${vaultName}/entities/${entityId}/refresh-from-source`,
        {
          method: "POST",
          body: JSON.stringify({
            metadata_only: options.metadata_only ?? false,
          }),
        },
      ),

    refreshCollectionFromSource: (collectionId: string) =>
      request<Job>(
        `/vaults/${vaultName}/collections/${collectionId}/refresh-from-source`,
        { method: "POST" },
      ),

    getMediaPreferences: () =>
      request<UserMediaPreferences>(
        `/vaults/${vaultName}/users/me/media-preferences`,
      ),

    setMediaPreferences: (prefs: UserMediaPreferences) =>
      request<UserMediaPreferences>(
        `/vaults/${vaultName}/users/me/media-preferences`,
        {
          method: "PUT",
          body: JSON.stringify(prefs),
        },
      ),

    getChannel: (channelId: string) =>
      request<ChannelDetail>(`/vaults/${vaultName}/channels/${channelId}`),

    listChannelVideos: (
      channelId: string,
      params: { limit?: number; sort?: string } = {},
    ) => {
      const query = new URLSearchParams();
      if (params.limit !== undefined) query.set("limit", String(params.limit));
      if (params.sort) query.set("sort", params.sort);
      const suffix = query.size > 0 ? `?${query.toString()}` : "";
      return request<EntityListItem[]>(
        `/vaults/${vaultName}/channels/${channelId}/videos${suffix}`,
      );
    },

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

    listLibraryLists: (
      params: {
        section?: string;
        source?: string;
        kind?: string;
      } = {},
    ) => {
      const query = new URLSearchParams();
      if (params.section) query.set("section", params.section);
      if (params.source) query.set("source", params.source);
      if (params.kind) query.set("kind", params.kind);
      const suffix = query.size > 0 ? `?${query.toString()}` : "";
      return request<LibraryListSummary[]>(
        `/vaults/${vaultName}/library/lists${suffix}`,
      );
    },

    getLibraryList: (
      listId: string,
      params: { sort?: string; section?: string; source?: string; kind?: string } = {},
    ) => {
      const query = new URLSearchParams();
      if (params.sort) query.set("sort", params.sort);
      if (params.section) query.set("section", params.section);
      if (params.source) query.set("source", params.source);
      if (params.kind) query.set("kind", params.kind);
      const suffix = query.size > 0 ? `?${query.toString()}` : "";
      return request<LibraryListDetail>(
        `/vaults/${vaultName}/library/lists/${encodeURIComponent(listId)}${suffix}`,
      );
    },

    getPlaybackState: (entityId: string) =>
      request<PlaybackStateResponse>(
        `/vaults/${vaultName}/playback-state/${entityId}`,
      ),

    upsertPlaybackState: (
      entityId: string,
      body: PlaybackStateInput,
    ) =>
      request<PlaybackStateResponse>(
        `/vaults/${vaultName}/playback-state/${entityId}`,
        {
          method: "PUT",
          body: JSON.stringify(body),
        },
      ),

    dismissPlaybackState: (entityId: string) =>
      request<{ entity_id: string; status: string }>(
        `/vaults/${vaultName}/playback-state/${entityId}/dismiss`,
        { method: "POST" },
      ),

    createUserList: (body: CreateUserListInput) =>
      request<{ id: string }>(`/vaults/${vaultName}/user-lists`, {
        method: "POST",
        body: JSON.stringify(body),
      }),

    addUserListMember: (listId: string, entityId: string) =>
      request<{ list_id: string; entity_id: string; status: string }>(
        `/vaults/${vaultName}/user-lists/${listId}/members`,
        {
          method: "POST",
          body: JSON.stringify({ entity_id: entityId }),
        },
      ),

    removeUserListMember: (listId: string, entityId: string) =>
      request<{ list_id: string; entity_id: string; status: string }>(
        `/vaults/${vaultName}/user-lists/${listId}/members/${entityId}`,
        { method: "DELETE" },
      ),

    reorderUserListMembers: (listId: string, entityIds: string[]) =>
      request<{ list_id: string; status: string }>(
        `/vaults/${vaultName}/user-lists/${listId}/members/reorder`,
        {
          method: "PATCH",
          body: JSON.stringify({ entity_ids: entityIds }),
        },
      ),

    addToWatchLater: async (entityId: string) => {
      const lists = await request<LibraryListSummary[]>(
        `/vaults/${vaultName}/library/lists?section=youtube`,
      );
      const watchLater = lists.find((list) => list.list_type === "watch_later");
      if (!watchLater?.id.startsWith("user:")) {
        throw new Error("Watch Later list not found");
      }
      const listId = watchLater.id.slice("user:".length);
      return request<{ list_id: string; entity_id: string; status: string }>(
        `/vaults/${vaultName}/user-lists/${listId}/members`,
        {
          method: "POST",
          body: JSON.stringify({ entity_id: entityId }),
        },
      );
    },

    listJobs: (params: {
      status?: string;
      type?: string;
      limit?: number;
      root_only?: boolean;
    } = {}) => {
      const query = new URLSearchParams();
      if (params.status) query.set("status", params.status);
      if (params.type) query.set("type", params.type);
      if (params.root_only) query.set("root_only", "true");
      query.set("limit", String(params.limit ?? 100));
      return request<Job[]>(`/vaults/${vaultName}/jobs?${query.toString()}`);
    },

    getJob: (jobId: string, params: { include_children?: boolean } = {}) => {
      const query = new URLSearchParams();
      if (params.include_children) query.set("include_children", "true");
      const suffix = query.size > 0 ? `?${query.toString()}` : "";
      return request<JobDetail>(`/vaults/${vaultName}/jobs/${jobId}${suffix}`);
    },

    cancelJob: (jobId: string) =>
      request<Job>(`/vaults/${vaultName}/jobs/${jobId}/cancel`, {
        method: "POST",
      }),

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

    runSubscription: (id: string) =>
      request<Job>(`/vaults/${vaultName}/subscriptions/${id}/run`, {
        method: "POST",
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
          asset_policy: body.asset_policy,
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
          asset_policy: body.asset_policy,
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
  channel_entity_id?: string | null;
  channel_avatar_preview?: EntityPreviewSummary | null;
  uploader?: string | null;
  duration?: string | null;
}

export interface ChannelDetail {
  id: string;
  title?: string | null;
  description?: string | null;
  follower_count?: string | null;
  verified?: boolean | null;
  source?: string | null;
  url?: string | null;
  avatar_preview?: EntityPreviewSummary | null;
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
  channel_entity_id?: string | null;
  channel_avatar_preview?: EntityPreviewSummary | null;
  uploader?: string | null;
  webpage_url?: string | null;
  playback_position?: number | null;
  playback_progress?: number | null;
}

export type LibraryListKind = "smart" | "user" | "source";

export interface LibraryListSummary {
  id: string;
  list_kind: LibraryListKind;
  list_type: string;
  title: string;
  member_count: number;
  cover_preview?: EntityPreviewSummary | null;
  icon?: string | null;
  overlay: boolean;
}

export interface LibraryListDetail {
  id: string;
  list_kind: LibraryListKind;
  list_type: string;
  title: string;
  member_count: number;
  cover_preview?: EntityPreviewSummary | null;
  icon?: string | null;
  overlay: boolean;
  members: CollectionMemberItem[];
  sort_options: string[];
  default_sort: string;
  can_reorder: boolean;
}

export interface PlaybackStateInput {
  asset_id: string;
  position_seconds: number;
  duration_seconds?: number | null;
}

export interface PlaybackStateResponse {
  entity_id: string;
  asset_id: string;
  position_seconds: number;
  duration_seconds?: number | null;
  completed_at?: string | null;
  updated_at: string;
}

export interface CreateUserListInput {
  title: string;
  list_type?: string;
}

export type BrowseSort =
  | "added_desc"
  | "published_desc"
  | "views_desc"
  | "manual"
  | "updated_desc";

export const BROWSE_SORT_LABELS: Record<BrowseSort, string> = {
  added_desc: "Date added",
  published_desc: "Publish date",
  views_desc: "View count",
  manual: "Manual order",
  updated_desc: "Recently updated",
};

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

export interface JobProgressStep {
  id: string;
  label: string;
  status: string;
  percent?: number | null;
}

export interface JobProgress {
  phase: string;
  current?: number | null;
  total?: number | null;
  label?: string | null;
  percent?: number | null;
  steps?: JobProgressStep[];
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
  progress?: JobProgress | null;
  parent_job_id?: string | null;
}

export interface JobDetail extends Job {
  children?: Job[];
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
  collection_id?: string | null;
  collection_title?: string | null;
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
