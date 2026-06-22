import { describe, expect, it } from "vitest";
import { getSettingsSections } from "./playerSettings";

describe("getSettingsSections", () => {
  it("always allows speed and hides empty track sections", () => {
    expect(getSettingsSections([])).toEqual({
      showSubtitles: false,
      showAudioTracks: false,
    });
  });

  it("shows subtitles when a present subtitle asset exists", () => {
    expect(
      getSettingsSections([
        {
          id: "1",
          role: "supporting",
          kind: "subtitle",
          size: 1,
          status: "present",
          storage_strategy: "managed",
          metadata: {},
        },
      ]),
    ).toEqual({
      showSubtitles: true,
      showAudioTracks: false,
    });
  });

  it("shows audio tracks only when multiple present audio assets exist", () => {
    const audio = {
      role: "supporting" as const,
      kind: "audio" as const,
      size: 1,
      status: "present" as const,
      storage_strategy: "managed" as const,
      metadata: {},
    };
    expect(
      getSettingsSections([
        { id: "a1", ...audio },
        { id: "a2", ...audio },
      ]),
    ).toEqual({
      showSubtitles: false,
      showAudioTracks: true,
    });
  });
});
