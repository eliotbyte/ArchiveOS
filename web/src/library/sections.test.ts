import { describe, expect, it } from "vitest";
import type { EntityListItem } from "../api/client";
import {
  COLLECTIONS_ENTRY,
  filterSectionItems,
  getSectionById,
  isYouTubeCollection,
  LIBRARY_SECTIONS,
  sectionBrowseParams,
} from "./sections";

describe("library sections registry", () => {
  it("defines the first-slice hardcoded sections", () => {
    expect(LIBRARY_SECTIONS.map((section) => section.id)).toEqual([
      "pictures",
      "videos",
      "youtube",
      "movies",
      "music",
    ]);
  });

  it("exposes stable routes for each section", () => {
    expect(getSectionById("pictures")?.route).toBe("/library/pictures");
    expect(getSectionById("music")?.route).toBe("/library/music");
    expect(COLLECTIONS_ENTRY.route).toBe("/playlists");
  });

  it("builds browse params from section filters", () => {
    const youtube = getSectionById("youtube")!;
    expect(sectionBrowseParams(youtube)).toEqual({
      limit: 100,
      kind: "video",
      source: "youtube",
    });

    const pictures = getSectionById("pictures")!;
    expect(sectionBrowseParams(pictures, "cat")).toEqual({
      limit: 100,
      kind: "image",
      query: "cat",
    });
  });

  it("filters excluded sources client-side", () => {
    const section = getSectionById("videos")!;
    const items: EntityListItem[] = [
      {
        id: "1",
        status: "present",
        tags: [],
        source: "youtube",
      },
      {
        id: "2",
        status: "present",
        tags: [],
        source: "inbox",
      },
    ];

    const filtered = filterSectionItems(items, section);
    expect(filtered).toHaveLength(1);
    expect(filtered[0]?.id).toBe("2");
  });

  it("recognizes youtube collection types", () => {
    expect(isYouTubeCollection("youtube_playlist")).toBe(true);
    expect(isYouTubeCollection("youtube_channel_uploads")).toBe(true);
    expect(isYouTubeCollection("folder")).toBe(false);
  });
});
