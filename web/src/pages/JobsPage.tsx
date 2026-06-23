import { useCallback, useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { ApiError, type Job, type JobProgressStep, type SourceFailure } from "../api/client";
import IntegrationIcon from "../components/IntegrationIcon";
import {
  formatFailureMessage,
  formatJobProgressSummary,
  formatJobStatus,
  formatStepSummary,
  isProblemJobStatus,
  isTerminalJobStatus,
  jobProgressPercent,
  summarizeFailureKinds,
} from "../integrations/jobs";
import { parseYtdlpJobInput, formatRelativeTime } from "../integrations/ytdlp";
import { useVaultApi } from "../context/VaultContext";

type JobTab = "active" | "problems";

const ACTIVE_STATUSES = new Set(["pending", "claimed", "running"]);

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
  const mode = parsed.mode === "subscription" ? "monitor sync" : "archive";
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
        : status === "unavailable"
          ? "unavailable"
        : status === "failed"
          ? "failed"
          : status === "skipped"
            ? "skipped"
          : "pending";
  const badgeClass =
    status === "unavailable" || status === "failed" ? "warn" : jobStatusClass(status);
  return <span className={`badge job-step-badge ${badgeClass}`}>{label}</span>;
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
      {step.message ? <div className="job-meta job-step-error">{step.message}</div> : null}
      {step.percent != null && step.status === "running" ? (
        <ProgressBar percent={step.percent} />
      ) : null}
    </li>
  );
}

function FailureRow({ failure }: { failure: SourceFailure }) {
  return (
    <li className="job-child-row">
      <div className="job-child-main">
        <span className="job-child-label">
          {failure.kind}/{failure.external_id}
        </span>
        <span className="badge warn">{failure.stage}</span>
      </div>
      <div className="job-meta job-step-error">{formatFailureMessage(failure)}</div>
      {failure.url ? (
        <div className="job-meta">
          <a href={failure.url} target="_blank" rel="noreferrer">
            {failure.url}
          </a>
        </div>
      ) : null}
    </li>
  );
}

function JobChildren({
  job,
  children,
  progressSteps,
  failures,
}: {
  job: Job;
  children: Job[];
  progressSteps: JobProgressStep[];
  failures: SourceFailure[];
}) {
  const childPreview = children.filter((child) => child.type === "preview");
  const failedSteps = progressSteps.filter((step) => step.status === "failed");
  const okSteps = progressSteps.filter((step) => step.status !== "failed");

  return (
    <ul className="job-children">
      {okSteps.map((step) => (
        <JobStepRow key={step.id} step={step} />
      ))}
      {failedSteps.map((step) => (
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
      {progressSteps.length === 0 && failures.length > 0
        ? failures.map((failure) => <FailureRow key={failure.id} failure={failure} />)
        : null}
      {progressSteps.length === 0 && childPreview.length === 0 && failures.length === 0 ? (
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
  const [tab, setTab] = useState<JobTab>("active");
  const [tabInitialized, setTabInitialized] = useState(false);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [expandedDetail, setExpandedDetail] = useState<{
    job: Job;
    children: Job[];
    failures: SourceFailure[];
  } | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);

  const activeJobs = useMemo(
    () => jobs.filter((job) => ACTIVE_STATUSES.has(job.status)),
    [jobs],
  );
  const problemJobs = useMemo(
    () => jobs.filter((job) => isProblemJobStatus(job.status)),
    [jobs],
  );

  const loadJobs = useCallback(
    async (silent = false) => {
      if (!silent) {
        setLoading(true);
      }
      setError(null);
      try {
        const jobItems = await api.listJobs({ limit: 200, root_only: true });
        setJobs(jobItems);
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
    if (tabInitialized || loading) return;
    if (problemJobs.length > 0) {
      setTab("problems");
    }
    setTabInitialized(true);
  }, [loading, problemJobs.length, tabInitialized]);

  const shouldPoll = useMemo(() => {
    if (tab === "active" && activeJobs.length > 0) return true;
    if (!expandedId || !expandedDetail) return false;
    const expandedRunning =
      expandedDetail.job.status === "running" || expandedDetail.job.status === "claimed";
    const childRunning = expandedDetail.children.some(
      (child) => child.status === "running" || child.status === "claimed",
    );
    return expandedRunning || childRunning;
  }, [tab, activeJobs.length, expandedId, expandedDetail]);

  useEffect(() => {
    if (!shouldPoll) return;
    const timer = window.setInterval(() => {
      void loadJobs(true);
    }, 3000);
    return () => window.clearInterval(timer);
  }, [shouldPoll, loadJobs]);

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
            failures: detail.failures ?? [],
          });
        }
      } catch {
        if (!cancelled) {
          const fallback = jobs.find((job) => job.id === expandedId);
          if (fallback) {
            setExpandedDetail({ job: fallback, children: [], failures: [] });
          }
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [api, expandedId, jobs]);

  const filtered = tab === "active" ? activeJobs : problemJobs;

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
      setTab("active");
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

  async function dismiss(jobId: string) {
    setBusyId(jobId);
    try {
      await api.deleteJob(jobId);
      if (expandedId === jobId) {
        setExpandedId(null);
      }
      await loadJobs();
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Dismiss failed");
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
            Active downloads and problem reports. Jobs with unavailable videos show
            as Partial. Successful jobs are removed automatically after 24 hours.
          </p>
        </div>
        <button className="secondary" type="button" onClick={() => void loadJobs()}>
          Refresh
        </button>
      </div>

      <div className="tab-row">
        {(["active", "problems"] as const).map((value) => (
          <button
            key={value}
            type="button"
            className={`tab${tab === value ? " active" : ""}`}
            onClick={() => setTab(value)}
          >
            {value}
            {value === "active" && activeJobs.length > 0 ? ` (${activeJobs.length})` : ""}
            {value === "problems" && problemJobs.length > 0
              ? ` (${problemJobs.length})`
              : ""}
          </button>
        ))}
      </div>

      {loading ? <div className="loading-state">Loading jobs...</div> : null}
      {error ? <div className="error-state">{error}</div> : null}

      <div className="job-list">
        {!loading && filtered.length === 0 ? (
          <p className="card-meta">
            {tab === "active"
              ? "No active downloads."
              : "No failed or partial jobs."}
          </p>
        ) : null}
        {filtered.map((job) => {
          const summary = summarizeYtdlpJob(job);
          const progressSummary = formatJobProgressSummary(job);
          const stepSummary = formatStepSummary(job);
          const overallPercent = jobProgressPercent(job.progress);
          const expanded = expandedId === job.id;
          const detail =
            expanded && expandedDetail?.job.id === job.id ? expandedDetail : null;
          const progressSteps = detail?.job.progress?.steps ?? job.progress?.steps ?? [];
          const jobFailures = detail?.failures ?? [];
          const failureSummary = summarizeFailureKinds(jobFailures);

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
                  {stepSummary ? <div className="job-meta">{stepSummary}</div> : null}
                  {failureSummary && !stepSummary ? (
                    <div className="job-meta">{failureSummary}</div>
                  ) : null}
                  {progressSummary && tab === "active" ? (
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
                <div className="job-item-actions">
                  {isProblemJobStatus(job.status) ? (
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
                  {isTerminalJobStatus(job.status) ? (
                    <button
                      className="secondary job-dismiss"
                      type="button"
                      disabled={busyId === job.id}
                      title="Remove this job from the list"
                      aria-label="Dismiss job"
                      onClick={() => void dismiss(job.id)}
                    >
                      ×
                    </button>
                  ) : null}
                </div>
              </div>
              {expanded ? (
                <JobChildren
                  job={detail?.job ?? job}
                  children={detail?.children ?? []}
                  progressSteps={progressSteps}
                  failures={jobFailures}
                />
              ) : null}
            </article>
          );
        })}
      </div>

      <p className="card-meta">
        Manage monitors on{" "}
        <Link to="/integrations/ytdlp">yt-dlp integration</Link>.
      </p>
    </section>
  );
}
