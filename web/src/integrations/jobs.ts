import type { Job, JobProgress } from "../api/client";

const STATUS_LABELS: Record<string, string> = {
  pending: "Queued",
  claimed: "Queued",
  running: "Running",
  partial: "Completed with errors",
  done: "Done",
  cancelled: "Cancelled",
};

const PHASE_LABELS: Record<string, string> = {
  discovering: "Discovering playlist",
  downloading: "Downloading",
  thumbnails: "Downloading thumbnails",
  importing: "Importing to library",
  preview: "Generating preview",
};

export function formatJobStatus(job: Job): string {
  if (job.type === "preview" && job.status === "running") {
    return "Generating preview";
  }
  return STATUS_LABELS[job.status] ?? job.status;
}

export function formatJobProgressSummary(job: Job): string | null {
  const progress = job.progress;
  if (!progress) return null;

  const phase = PHASE_LABELS[progress.phase] ?? progress.phase;

  if (progress.phase === "downloading" && progress.current != null && progress.total != null) {
    const label = progress.label ? ` — ${progress.label}` : "";
    const pct =
      progress.percent != null ? ` (${Math.round(progress.percent)}%)` : "";
    return `${phase} ${progress.current}/${progress.total}${label}${pct}`;
  }

  if (progress.current != null && progress.total != null) {
    return `${phase} ${progress.current}/${progress.total}`;
  }

  if (progress.label) {
    return `${phase}: ${progress.label}`;
  }

  if (progress.percent != null) {
    return `${phase} (${Math.round(progress.percent)}%)`;
  }

  return phase;
}

export function jobProgressPercent(progress?: JobProgress | null): number | null {
  if (!progress) return null;
  if (progress.percent != null) return Math.min(100, Math.max(0, progress.percent));
  if (progress.current != null && progress.total != null && progress.total > 0) {
    return (progress.current / progress.total) * 100;
  }
  return null;
}

export function countStepStatuses(progress?: JobProgress | null): {
  done: number;
  failed: number;
  total: number;
} | null {
  if (!progress?.steps?.length) return null;
  const total = progress.steps.length;
  const done = progress.steps.filter((s) => s.status === "done").length;
  const failed = progress.steps.filter((s) => s.status === "failed").length;
  return { done, failed, total };
}
