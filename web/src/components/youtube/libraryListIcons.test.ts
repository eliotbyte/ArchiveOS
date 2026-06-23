import { describe, expect, it } from "vitest";
import { libraryListMaterialIcon } from "./libraryListIcons";

describe("libraryListMaterialIcon", () => {
  it("maps smart and user list types to Material Symbols", () => {
    expect(libraryListMaterialIcon("smart_recently_watched")).toBe("history");
    expect(libraryListMaterialIcon("smart_recently_added")).toBe(
      "hourglass_arrow_down",
    );
    expect(libraryListMaterialIcon("watch_later")).toBe("schedule");
    expect(libraryListMaterialIcon("youtube_playlist")).toBeNull();
  });
});
