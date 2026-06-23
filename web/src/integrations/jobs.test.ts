import { describe, expect, it } from "vitest";
import type { Job, SourceFailure } from "../api/client";
import {
  formatFailureMessage,
  formatJobProgressSummary,
  formatJobStatus,
  formatStepSummary,
  isProblemJobStatus,
  isTerminalJobStatus,
  jobProgressPercent,
} from "./jobs";

describe("job formatting", () => {
  it("maps statuses to readable labels", () => {
    expect(formatJobStatus({ status: "partial", type: "yt-dlp" } as Job)).toBe(
      "Partial",
    );
    expect(formatJobStatus({ status: "failed", type: "yt-dlp" } as Job)).toBe(
      "Failed",
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

  it("summarizes finished step counts", () => {
    const job = {
      status: "partial",
      type: "yt-dlp",
      progress: {
        phase: "finished",
        steps: [
          { id: "a", label: "A", status: "done" },
          { id: "b", label: "B", status: "failed" },
        ],
      },
    } as Job;
    expect(formatStepSummary(job)).toBe("1/2 OK, 1 failed");
  });

  it("humanizes probe failures", () => {
    const failure = {
      id: "1",
      source: "youtube",
      kind: "video",
      external_id: "job",
      stage: "probe",
      error_kind: "unknown",
      message: "HTTP Error 404: Not Found",
      retryable: false,
      created_at: "2024-01-01T00:00:00Z",
    } as SourceFailure;
    expect(formatFailureMessage(failure)).toBe("Playlist or channel not found");
  });

  it("classifies problem and terminal statuses", () => {
    expect(isProblemJobStatus("partial")).toBe(true);
    expect(isProblemJobStatus("failed")).toBe(true);
    expect(isProblemJobStatus("running")).toBe(false);
    expect(isTerminalJobStatus("done")).toBe(true);
    expect(isTerminalJobStatus("running")).toBe(false);
  });
});
