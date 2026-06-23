import { describe, expect, it } from "vitest";
import {
  formatIntervalMinutes,
  isYtdlpSource,
  parseYtdlpJobInput,
} from "./ytdlp";

describe("ytdlp helpers", () => {
  it("detects yt-dlp extractor sources", () => {
    expect(isYtdlpSource("youtube")).toBe(true);
    expect(isYtdlpSource("youtubetab")).toBe(true);
    expect(isYtdlpSource("local")).toBe(false);
  });

  it("parses structured job input", () => {
    const parsed = parseYtdlpJobInput(
      JSON.stringify({
        url: "https://youtube.com/watch?v=abc",
        mode: "subscription",
      }),
    );
    expect(parsed?.mode).toBe("subscription");
    expect(parsed?.url).toContain("youtube.com");
  });

  it("formats interval minutes", () => {
    expect(formatIntervalMinutes(60)).toBe("every hour");
    expect(formatIntervalMinutes(600)).toBe("every 10 hours");
  });
});
