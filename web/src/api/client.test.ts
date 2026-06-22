import { describe, expect, it } from "vitest";
import {
  browserDisplayState,
  kindIcon,
  mediaKindFromEntity,
  previewVisualState,
  type EntityPreviewSummary,
} from "./client";

describe("browserDisplayState", () => {
  it("marks common browser media as supported", () => {
    expect(browserDisplayState("image/jpeg")).toBe("supported");
    expect(browserDisplayState("video/mp4")).toBe("supported");
    expect(browserDisplayState("application/pdf")).toBe("supported");
  });

  it("marks exotic media as unsupported", () => {
    expect(browserDisplayState("application/octet-stream")).toBe("unsupported");
  });
});

describe("previewVisualState", () => {
  const preview: EntityPreviewSummary = {
    entity_id: "e1",
    asset_id: "a1",
    kind: "thumbnail",
    preview_role: "source_thumbnail",
    status: "present",
  };

  it("returns missing without preview", () => {
    expect(previewVisualState(null)).toBe("missing");
  });

  it("returns pending for non-present preview assets", () => {
    expect(
      previewVisualState({ ...preview, status: "remote" }),
    ).toBe("pending");
  });

  it("returns failed when image load fails", () => {
    expect(previewVisualState(preview, true)).toBe("failed");
  });
});

describe("kindIcon", () => {
  it("returns icons per kind", () => {
    expect(kindIcon("video")).toBe("▶");
    expect(kindIcon("image")).toBe("◻");
  });
});

describe("mediaKindFromEntity", () => {
  it("infers image from mime when kind is file", () => {
    expect(mediaKindFromEntity("file", "image/jpeg")).toBe("image");
    expect(mediaKindFromEntity(null, "video/mp4")).toBe("video");
  });
});
