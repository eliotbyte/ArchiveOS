import { describe, expect, it } from "vitest";
import {
  DRAG_THRESHOLD_PX,
  pointerMovedBeyondThreshold,
  snapMiniCorner,
} from "./miniPlayerSnap";

describe("miniPlayerSnap", () => {
  it("snaps to viewport quadrants", () => {
    expect(snapMiniCorner(100, 100, 800, 600)).toBe("top-left");
    expect(snapMiniCorner(700, 100, 800, 600)).toBe("top-right");
    expect(snapMiniCorner(100, 500, 800, 600)).toBe("bottom-left");
    expect(snapMiniCorner(700, 500, 800, 600)).toBe("bottom-right");
  });

  it("detects drag when movement exceeds threshold", () => {
    expect(pointerMovedBeyondThreshold(0, 0, 6, 0)).toBe(true);
    expect(pointerMovedBeyondThreshold(0, 0, 0, 6)).toBe(true);
    expect(pointerMovedBeyondThreshold(0, 0, 3, 3)).toBe(false);
    expect(DRAG_THRESHOLD_PX).toBe(5);
  });
});
