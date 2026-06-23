import { useCallback, useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { ApiError, type Job, type JobProgressStep, type SourceFailure } from "../api/client";
import IntegrationIcon from "../components/IntegrationIcon";
import {
  formatJobProgressSummary,
  formatJobStatus,
  jobProgressPercent,
} from "../integrations/jobs";
import { parseYtdlpJobInput, formatRelativeTime } from "../integrations/ytdlp";
import { useVaultApi } from "../context/VaultContext";

type JobTab = "active" | "failed" | "done" | "failures" | "all";

const ACTIVE_STATUSES = new Set(["pending", "claimed", "running", "partial"]);
const DONE_STATUSES = new Set(["done", "partial", "cancelled"]);

function jobStatusClass(status: string): string {
  switch (status) {
    case "done":
      return "ok";
    case "failed":
      return "warn";
    case "partial":
      return "warn";
    case "cancelled":
      return "";
    case "running":
    case "claimed":
      return "ok";
    default:
      return "";
  }
}

function summarizeYtdlpJob(job: Job): string | null {
  if (job.type !== "yt-dlp") return null;
  const parsed = parseYtdlpJobInput(job.input);
  if (!parsed) return job.input.slice(0, 120);
  const mode =
    parsed.mode === "subscription" ? "monitor sync" : "archive";
  try {
    const host = new URL(parsed.url).hostname;
    return `${mode} · ${host}`;
  } catch {
    return `${mode} · ${parsed.url.slice(0, 80)}`;
  }
}

function StepStatusBadge({ status }: { status: string }) {
  const label =
    status === "running"
      ? "downloading"
      : status === "done"
        ? "done"
        : status === "failed"
          ? "failed"
          : "pending";
  return <span className={`badge job-step-badge ${jobStatusClass(status)}`}>{label}</span>;
}

function ProgressBar({ percent }: { percent: number }) {
  return (
    <div className="job-progress-track" aria-hidden>
      <div
        className="job-progress-fill"
        style={{ width: `${Math.min(100, Math.max(0, percent))}%` }}
      />
    </div>
  );
}

function JobStepRow({ step }: { step: JobProgressStep }) {
  return (
    <li className="job-child-row">
      <div className="job-child-main">
        <span className="job-child-label">{step.label}</span>
        <StepStatusBadge status={step.status} />
      </div>
      {step.percent != null && step.status === "running" ? (
        <ProgressBar percent={step.percent} />
      ) : null}
    </li>
  );
}

function JobChildren({
  job,
  children,
  progressSteps,
}: {
  job: Job;
  children: Job[];
  progressSteps: JobProgressStep[];
}) {
  const childPreview = children.filter((child) => child.type === "preview");

  return (
    <ul className="job-children">
      {progressSteps.map((step) => (
        <JobStepRow key={step.id} step={step} />
      ))}
      {childPreview.map((child) => (
        <li className="job-child-row" key={child.id}>
          <div className="job-child-main">
            <span className="job-child-label">
              {formatJobProgressSummary(child) ?? "Preview generation"}
            </span>
            <span className={`badge ${jobStatusClass(child.status)}`}>
              {formatJobStatus(child)}
            </span>
          </div>
          {child.progress?.percent != null ? (
            <ProgressBar percent={child.progress.percent} />
          ) : null}
        </li>
      ))}
      {progressSteps.length === 0 && childPreview.length === 0 ? (
        <li className="job-child-row job-child-empty">
          {job.status === "running" || job.status === "claimed"
            ? "Waiting for progress updates…"
            : "No step details recorded."}
        </li>
      ) : null}
    </ul>
  );
}

export default function JobsPage() {
  const api = useVaultApi();
  const [jobs, setJobs] = useState<Job[]>([]);
  const [failures, setFailures] = useState<SourceFailure[]>([]);
  const [tab, setTab] = useState<JobTab>("active");
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [expandedDetail, setExpandedDetail] = useState<{
    job: Job;
    children: Job[];
  } | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);

  const loadJobs = useCallback(
    async (silent = false) => {
      if (!silent) {
        setLoading(true);
      }
      setError(null);
      try {
        const [jobItems, failureItems] = await Promise.all([
          api.listJobs({ limit: 200, root_only: true }),
          api.listSourceFailures(),
        ]);
        setJobs(jobItems);
        setFailures(failureItems);
      } catch (err) {
        setError(err instanceof ApiError ? err.message : "Failed to load jobs");
      } finally {
        if (!silent) {
          setLoading(false);
        }
      }
    },
    [api],
  );

  useEffect(() => {
    void loadJobs();
  }, [loadJobs]);

  useEffect(() => {
    if (tab !== "active") return;
    const timer = window.setInterval(() => {
      void loadJobs(true);
    }, 3000);
    return () => window.clearInterval(timer);
  }, [tab, loadJobs]);

  useEffect(() => {
    if (!expandedId) {
      setExpandedDetail(null);
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const detail = await api.getJob(expandedId, { include_children: true });
        if (!cancelled) {
          setExpandedDetail({
            job: detail,
            children: detail.children ?? [],
          });
        }
      } catch {
        if (!cancelled) {
          const fallback = jobs.find((job) => job.id === expandedId);
          if (fallback) {
            setExpandedDetail({ job: fallback, children: [] });
          }
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [api, expandedId, jobs]);

  const filtered = useMemo(() => {
    switch (tab) {
      case "active":
        return jobs.filter((job) => ACTIVE_STATUSES.has(job.status));
      case "failed":
        return jobs.filter((job) => job.status === "failed");
      case "done":
        return jobs.filter((job) => DONE_STATUSES.has(job.status));
      case "failures":
        return [];
      default:
        return jobs;
    }
  }, [jobs, tab]);

  async function toggleExpand(job: Job) {
    if (expandedId === job.id) {
      setExpandedId(null);
      return;
    }
    setExpandedId(job.id);
  }

  async function retry(jobId: string) {
    setBusyId(jobId);
    try {
      await api.retryJob(jobId);
      await loadJobs();
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Requeue failed");
    } finally {
      setBusyId(null);
    }
  }

  async function cancel(jobId: string) {
    setBusyId(jobId);
    try {
      await api.cancelJob(jobId);
      await loadJobs();
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Cancel failed");
    } finally {
      setBusyId(null);
    }
  }

  return (
    <section className="panel">
      <div className="detail-header">
        <div>
          <h2>Jobs</h2>
          <p className="subtitle">
            Background downloads and preview generation. Videos appear in the
            library only after a job finishes importing — not while tracks are
            still downloading.
          </p>
        </div>
        <button className="secondary" type="button" onClick={() => void loadJobs()}>
          Refresh
        </button>
      </div>

      <div className="tab-row">
        {(["active", "failed", "done", "failures", "all"] as const).map((value) => (
          <button
            key={value}
            type="button"
            className={`tab${tab === value ? " active" : ""}`}
            onClick={() => setTab(value)}
          >
            {value}
            {value === "failures" && failures.length > 0
              ? ` (${failures.length})`
              : ""}
          </button>
        ))}
      </div>

      {loading ? <div className="loading-state">Loading jobs...</div> : null}
      {error ? <div className="error-state">{error}</div> : null}

      {tab === "failures" ? (
        <div className="job-list">
          {failures.length === 0 ? (
            <p className="card-meta">No recorded source import failures.</p>
          ) : (
            failures.map((failure) => (
              <article className="job-item" key={failure.id}>
                <div>
                  <strong>
                    {failure.source}/{failure.kind}
                  </strong>
                  <div className="job-meta">{failure.message}</div>
                  <div className="job-meta">
                    {failure.stage} · {failure.error_kind}
                    {failure.retryable ? " · retryable" : ""} ·{" "}
                    {formatRelativeTime(failure.created_at)}
                  </div>
                  {failure.url ? (
                    <div className="job-meta">{failure.url}</div>
                  ) : null}
                </div>
              </article>
            ))
          )}
        </div>
      ) : (
        <div className="job-list">
          {filtered.map((job) => {
            const summary = summarizeYtdlpJob(job);
            const progressSummary = formatJobProgressSummary(job);
            const overallPercent = jobProgressPercent(job.progress);
            const expanded = expandedId === job.id;
            const detail =
              expanded && expandedDetail?.job.id === job.id
                ? expandedDetail
                : null;
            const progressSteps = detail?.job.progress?.steps ?? job.progress?.steps ?? [];

            return (
              <article className={`job-item${expanded ? " expanded" : ""}`} key={job.id}>
                <div className="job-item-main">
                  <button
                    className={`job-expand${expanded ? " open" : ""}`}
                    type="button"
                    aria-expanded={expanded}
                    aria-label={expanded ? "Collapse job details" : "Expand job details"}
                    onClick={() => void toggleExpand(job)}
                  >
                    ▶
                  </button>
                  <div className="job-item-body">
                    <div className="job-row-title">
                      {job.type === "yt-dlp" ? (
                        <IntegrationIcon integration="ytdlp" size="sm" />
                      ) : null}
                      <strong>{job.type}</strong>
                      <span className={`badge ${jobStatusClass(job.status)}`}>
                        {formatJobStatus(job)}
                      </span>
                    </div>
                    {summary ? <div className="job-meta">{summary}</div> : null}
                    {progressSummary ? (
                      <div className="job-meta job-progress-summary">{progressSummary}</div>
                    ) : null}
                    {overallPercent != null &&
                    (job.status === "running" || job.status === "claimed") ? (
                      <ProgressBar percent={overallPercent} />
                    ) : null}
                    <div className="job-meta">
                      attempts {job.attempts} · {formatRelativeTime(job.created_at)}
                    </div>
                  </div>
                  {job.status === "failed" ? (
                    <button
                      className="secondary"
                      type="button"
                      disabled={busyId === job.id}
                      title="Requeues the job without checking whether the upstream issue is resolved"
                      onClick={() => void retry(job.id)}
                    >
                      {busyId === job.id ? "Requeuing..." : "Requeue"}
                    </button>
                  ) : null}
                  {job.status === "running" ||
                  job.status === "claimed" ||
                  job.status === "pending" ? (
                    <button
                      className="secondary"
                      type="button"
                      disabled={busyId === job.id}
                      onClick={() => void cancel(job.id)}
                    >
                      {busyId === job.id ? "Cancelling..." : "Cancel"}
                    </button>
                  ) : null}
                </div>
                {expanded ? (
                  <JobChildren
                    job={detail?.job ?? job}
                    children={detail?.children ?? []}
                    progressSteps={progressSteps}
                  />
                ) : null}
              </article>
            );
          })}
        </div>
      )}

      <p className="card-meta">
        Manage monitors on{" "}
        <Link to="/integrations/ytdlp">yt-dlp integration</Link>.
      </p>
    </section>
  );
}
