import { describe, expect, it } from "vitest";
import type { Job } from "../api/client";
import {
  formatJobProgressSummary,
  formatJobStatus,
  jobProgressPercent,
} from "./jobs";

describe("job formatting", () => {
  it("maps statuses to readable labels", () => {
    expect(formatJobStatus({ status: "partial", type: "yt-dlp" } as Job)).toBe(
      "Completed with errors",
    );
    expect(formatJobStatus({ status: "running", type: "preview" } as Job)).toBe(
      "Generating preview",
    );
  });

  it("summarizes download progress", () => {
    const job = {
      status: "running",
      type: "yt-dlp",
      progress: {
        phase: "downloading",
        current: 3,
        total: 10,
        label: "Cool video",
        percent: 47.2,
      },
    } as Job;
    expect(formatJobProgressSummary(job)).toBe(
      "Downloading 3/10 — Cool video (47%)",
    );
    expect(jobProgressPercent(job.progress)).toBe(47.2);
  });
});
