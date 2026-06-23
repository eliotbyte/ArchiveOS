/** Platform extractors handled by the yt-dlp worker (source_ref.source values). */
export const YTDLP_EXTRACTOR_SOURCES = new Set([
  "youtube",
  "vimeo",
  "pornhub",
  "twitter",
  "tiktok",
  "twitch",
]);

export function isYtdlpSource(source?: string | null): boolean {
  if (!source) return false;
  const normalized = source.toLowerCase();
  if (normalized === "youtube" || normalized.startsWith("youtube")) {
    return true;
  }
  return YTDLP_EXTRACTOR_SOURCES.has(normalized);
}

export interface ParsedYtdlpJobInput {
  url: string;
  mode: "once" | "subscription";
  resync?: boolean;
}

export function parseYtdlpJobInput(raw: string): ParsedYtdlpJobInput | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;
  if (trimmed.startsWith("{")) {
    try {
      const payload = JSON.parse(trimmed) as {
        url?: string;
        mode?: string;
      };
      if (!payload.url) return null;
      return {
        url: payload.url,
        mode: payload.mode === "subscription" ? "subscription" : "once",
      };
    } catch {
      return null;
    }
  }
  return { url: trimmed, mode: "once" };
}

export function formatIntervalMinutes(minutes: number): string {
  if (minutes < 60) return `every ${minutes} min`;
  if (minutes % 60 === 0) {
    const hours = minutes / 60;
    return hours === 1 ? "every hour" : `every ${hours} hours`;
  }
  const hours = Math.floor(minutes / 60);
  const mins = minutes % 60;
  return `every ${hours}h ${mins}m`;
}

export function formatRelativeTime(iso: string): string {
  const target = new Date(iso).getTime();
  if (Number.isNaN(target)) return iso;
  const diffMs = target - Date.now();
  const abs = Math.abs(diffMs);
  const minutes = Math.round(abs / 60_000);
  if (minutes < 60) {
    return diffMs >= 0 ? `in ${minutes}m` : `${minutes}m ago`;
  }
  const hours = Math.round(minutes / 60);
  if (hours < 48) {
    return diffMs >= 0 ? `in ${hours}h` : `${hours}h ago`;
  }
  const days = Math.round(hours / 24);
  return diffMs >= 0 ? `in ${days}d` : `${days}d ago`;
}

export const MONITOR_INTERVAL_PRESETS = [
  { label: "Every hour", minutes: 60 },
  { label: "Every 6 hours", minutes: 360 },
  { label: "Every 12 hours", minutes: 720 },
  { label: "Every 24 hours", minutes: 1440 },
  { label: "Custom", minutes: -1 },
] as const;

export const LANGUAGE_OPTIONS = [
  { value: "original", label: "Original" },
  { value: "ru", label: "Russian" },
  { value: "en", label: "English" },
  { value: "de", label: "German" },
  { value: "fr", label: "French" },
  { value: "es", label: "Spanish" },
  { value: "ja", label: "Japanese" },
  { value: "zh", label: "Chinese" },
] as const;

export function languageLabel(code: string): string {
  return LANGUAGE_OPTIONS.find((o) => o.value === code)?.label ?? code;
}
