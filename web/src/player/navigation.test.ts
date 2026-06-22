import { beforeEach, describe, expect, it } from "vitest";
import { shouldDeferEntityNavigation } from "./navigation";

describe("shouldDeferEntityNavigation", () => {
  beforeEach(() => {
    Object.defineProperty(document, "fullscreenElement", {
      configurable: true,
      value: null,
    });
  });

  it("returns false when not in fullscreen", () => {
    expect(shouldDeferEntityNavigation()).toBe(false);
  });

  it("returns true when a fullscreen element is active", () => {
    const element = document.createElement("div");
    Object.defineProperty(document, "fullscreenElement", {
      configurable: true,
      value: element,
    });
    try {
      expect(shouldDeferEntityNavigation()).toBe(true);
    } finally {
      Object.defineProperty(document, "fullscreenElement", {
        configurable: true,
        value: null,
      });
    }
  });
});
