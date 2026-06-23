import type { EntityListItem } from "../api/client";
import { isYtdlpSource } from "../integrations/ytdlp";

export type SectionViewMode = "grid" | "gallery" | "youtube";

export interface SectionBrowseFilter {
  kind?: string;
  source?: string;
  /** Applied client-side when the API has no exclude filter. */
  excludeSource?: string;
}

export interface LibrarySection {
  id: string;
  label: string;
  route: string;
  description: string;
  browse: SectionBrowseFilter;
  viewMode: SectionViewMode;
  placeholder?: boolean;
}

export const YOUTUBE_COLLECTION_TYPES = [
  "youtube_playlist",
  "youtube_channel_uploads",
] as const;

export const LIBRARY_SECTIONS: LibrarySection[] = [
  {
    id: "pictures",
    label: "Pictures",
    route: "/library/pictures",
    description: "Browse images in a gallery layout.",
    browse: { kind: "image" },
    viewMode: "gallery",
  },
  {
    id: "videos",
    label: "Videos",
    route: "/library/videos",
    description: "Local and imported videos outside YouTube.",
    browse: { kind: "video", excludeSource: "youtube" },
    viewMode: "grid",
  },
  {
    id: "youtube",
    label: "YouTube",
    route: "/library/youtube",
    description: "Archived YouTube videos, playlists, and channels.",
    browse: { kind: "video", source: "youtube" },
    viewMode: "youtube",
  },
  {
    id: "movies",
    label: "Movies & Shows",
    route: "/library/movies",
    description: "Films and series. Filtering by type is coming soon.",
    browse: { kind: "video" },
    viewMode: "grid",
    placeholder: true,
  },
  {
    id: "music",
    label: "Music",
    route: "/library/music",
    description: "Audio tracks and recordings.",
    browse: { kind: "audio" },
    viewMode: "grid",
  },
];

export const COLLECTIONS_ENTRY = {
  id: "collections",
  label: "Collections",
  route: "/playlists",
  description: "Playlists, channels, folders, and user lists.",
} as const;

export function getSectionById(sectionId: string): LibrarySection | undefined {
  return LIBRARY_SECTIONS.find((section) => section.id === sectionId);
}

export function sectionBrowseParams(
  section: LibrarySection,
  query = "",
): Record<string, string | number> {
  const params: Record<string, string | number> = { limit: 100 };
  if (query) params.query = query;
  if (section.browse.kind) params.kind = section.browse.kind;
  if (section.browse.source) params.source = section.browse.source;
  if (section.browse.excludeSource) {
    params.exclude_source = section.browse.excludeSource;
  }
  return params;
}

export function filterSectionItems(
  items: EntityListItem[],
  section: LibrarySection,
): EntityListItem[] {
  if (!section.browse.excludeSource) return items;
  return items.filter((item) => !isYtdlpSource(item.source));
}

export function isYouTubeCollection(collectionType: string): boolean {
  return (YOUTUBE_COLLECTION_TYPES as readonly string[]).includes(collectionType);
}
