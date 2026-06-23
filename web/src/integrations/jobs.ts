import type { Job, JobProgress, SourceFailure } from "../api/client";

const STATUS_LABELS: Record<string, string> = {
  pending: "Queued",
  claimed: "Queued",
  running: "Running",
  partial: "Partial",
  failed: "Failed",
  done: "Done",
  cancelled: "Cancelled",
};

const PHASE_LABELS: Record<string, string> = {
  discovering: "Discovering playlist",
  downloading: "Downloading",
  thumbnails: "Downloading thumbnails",
  avatars: "Downloading avatars",
  importing: "Importing to library",
  preview: "Generating preview",
  finished: "Finished",
};

const ERROR_KIND_LABELS: Record<string, string> = {
  private: "private",
  dead: "deleted",
  unavailable: "unavailable",
  region_locked: "region locked",
  no_formats: "no formats",
  network: "network error",
  validation_failed: "validation failed",
  unknown: "unknown",
};

const TERMINAL_STATUSES = new Set(["done", "partial", "failed", "cancelled"]);

export function isTerminalJobStatus(status: string): boolean {
  return TERMINAL_STATUSES.has(status);
}

export function isProblemJobStatus(status: string): boolean {
  return status === "failed" || status === "partial";
}

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

  if (progress.phase === "finished" && progress.label) {
    return progress.label;
  }

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

export function formatStepSummary(job: Job): string | null {
  const counts = countStepStatuses(job.progress);
  if (!counts || counts.total === 0) return null;
  if (counts.failed === 0) {
    return `${counts.done}/${counts.total} succeeded`;
  }
  return `${counts.done}/${counts.total} OK, ${counts.failed} failed`;
}

export function formatFailureMessage(failure: SourceFailure): string {
  const kind = failure.kind;
  const errorKind = ERROR_KIND_LABELS[failure.error_kind] ?? failure.error_kind;

  if (failure.stage === "probe") {
    const text = failure.message.toLowerCase();
    if (text.includes("404") || text.includes("not found") || text.includes("does not exist")) {
      return "Playlist or channel not found";
    }
    return failure.message;
  }

  if (kind === "video") {
    if (failure.error_kind === "dead") return "Video was deleted";
    if (failure.error_kind === "unavailable") return "Video unavailable";
    if (failure.error_kind === "private") return "Video is private";
    if (failure.error_kind === "region_locked") return "Video blocked in your region";
    return `Video ${errorKind}: ${failure.message}`;
  }

  if (kind === "thumbnail") {
    return `Thumbnail ${errorKind}: ${failure.message}`;
  }

  if (kind === "avatar") {
    return `Channel avatar ${errorKind}: ${failure.message}`;
  }

  return failure.message;
}

export function summarizeFailureKinds(failures: SourceFailure[]): string | null {
  if (failures.length === 0) return null;
  const kinds = new Map<string, number>();
  for (const failure of failures) {
    const key = failure.error_kind || "unknown";
    kinds.set(key, (kinds.get(key) ?? 0) + 1);
  }
  const parts = [...kinds.entries()].map(([kind, count]) => {
    const label = ERROR_KIND_LABELS[kind] ?? kind;
    return count > 1 ? `${count}× ${label}` : label;
  });
  return parts.join(", ");
}
