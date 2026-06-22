export const API_BASE =
  import.meta.env.VITE_API_BASE ?? (import.meta.env.DEV ? "/api" : "");
export const VAULT_NAME = import.meta.env.VITE_VAULT_NAME ?? "archiveos";

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

export function assetContentUrl(assetId: string): string {
  return `${API_BASE}/vaults/${VAULT_NAME}/assets/${assetId}/content`;
}

export const api = {
  listEntities: (params: Record<string, string | number | boolean | undefined> = {}) => {
    const query = new URLSearchParams();
    for (const [key, value] of Object.entries(params)) {
      if (value !== undefined && value !== "") {
        query.set(key, String(value));
      }
    }
    const suffix = query.size > 0 ? `?${query.toString()}` : "";
    return request<EntityListItem[]>(`/vaults/${VAULT_NAME}/entities${suffix}`);
  },

  getEntity: (id: string) =>
    request<EntityDetail>(`/vaults/${VAULT_NAME}/entities/${id}`),

  searchEntities: (query: string) =>
    request<EntityHit[]>(
      `/vaults/${VAULT_NAME}/search?${new URLSearchParams({ query }).toString()}`,
    ),

  listJobs: (limit = 50) =>
    request<Job[]>(`/vaults/${VAULT_NAME}/jobs?limit=${limit}`),

  retryJob: (jobId: string) =>
    request<Job>(`/vaults/${VAULT_NAME}/jobs/${jobId}/retry`, { method: "POST" }),

  archiveUrl: (url: string) =>
    request<Job>(`/vaults/${VAULT_NAME}/archive`, {
      method: "POST",
      body: JSON.stringify({
        url,
        asset_policy: {
          video: "best",
          thumbnail: true,
          subtitles: "preferred",
          subtitle_languages: ["original", "en", "ru"],
          automatic_subtitles: true,
          audio_tracks: "main",
        },
      }),
    }),

  acquireAsset: (entityId: string, assetId: string) =>
    request<Job>(
      `/vaults/${VAULT_NAME}/entities/${entityId}/assets/${assetId}/acquire`,
      { method: "POST" },
    ),
};

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
  primary_asset_id?: string | null;
  primary_asset_status?: string | null;
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
}

export interface Job {
  id: string;
  type: string;
  status: string;
  input: string;
  attempts: number;
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
