import { useEffect, useMemo, useState } from "react";
import { ApiError, type Job } from "../api/client";
import { useVaultApi } from "../context/VaultContext";

type JobTab = "active" | "failed" | "done" | "all";

const ACTIVE_STATUSES = new Set(["pending", "claimed", "running", "partial"]);

export default function JobsPage() {
  const api = useVaultApi();
  const [jobs, setJobs] = useState<Job[]>([]);
  const [tab, setTab] = useState<JobTab>("active");
  const [selectedJob, setSelectedJob] = useState<Job | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);

  async function loadJobs() {
    setLoading(true);
    setError(null);
    try {
      setJobs(await api.listJobs({ limit: 200 }));
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Failed to load jobs");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void loadJobs();
  }, [api]);

  const filtered = useMemo(() => {
    switch (tab) {
      case "active":
        return jobs.filter((job) => ACTIVE_STATUSES.has(job.status));
      case "failed":
        return jobs.filter((job) => job.status === "failed");
      case "done":
        return jobs.filter((job) => job.status === "done");
      default:
        return jobs;
    }
  }, [jobs, tab]);

  async function openJob(job: Job) {
    try {
      setSelectedJob(await api.getJob(job.id));
    } catch {
      setSelectedJob(job);
    }
  }

  async function retry(jobId: string) {
    setBusyId(jobId);
    try {
      await api.retryJob(jobId);
      await loadJobs();
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Retry failed");
    } finally {
      setBusyId(null);
    }
  }

  return (
    <section className="panel">
      <div className="detail-header">
        <div>
          <h2>Jobs</h2>
          <p className="subtitle">Background work across ingestion and preview generation.</p>
        </div>
        <button className="secondary" type="button" onClick={() => void loadJobs()}>
          Refresh
        </button>
      </div>

      <div className="tab-row">
        {(["active", "failed", "done", "all"] as const).map((value) => (
          <button
            key={value}
            type="button"
            className={`tab${tab === value ? " active" : ""}`}
            onClick={() => setTab(value)}
          >
            {value}
          </button>
        ))}
      </div>

      {loading ? <div className="loading-state">Loading jobs...</div> : null}
      {error ? <div className="error-state">{error}</div> : null}

      <div className="job-list">
        {filtered.map((job) => (
          <article className="job-item" key={job.id}>
            <button className="job-summary" type="button" onClick={() => void openJob(job)}>
              <strong>{job.type}</strong>
              <div className="job-meta">
                {job.status} · attempts {job.attempts} · {job.created_at}
              </div>
              <div className="job-meta">{job.input.slice(0, 160)}</div>
            </button>
            {job.status === "failed" ? (
              <button
                className="secondary"
                type="button"
                disabled={busyId === job.id}
                onClick={() => void retry(job.id)}
              >
                {busyId === job.id ? "Retrying..." : "Retry"}
              </button>
            ) : null}
          </article>
        ))}
      </div>

      {selectedJob ? (
        <aside className="job-drawer panel">
          <div className="detail-header">
            <h3>{selectedJob.type}</h3>
            <button className="secondary" type="button" onClick={() => setSelectedJob(null)}>
              Close
            </button>
          </div>
          <p className="card-meta">Status: {selectedJob.status}</p>
          <p className="card-meta">Attempts: {selectedJob.attempts}</p>
          <p className="card-meta">Created: {selectedJob.created_at}</p>
          <pre className="job-input">{selectedJob.input}</pre>
        </aside>
      ) : null}
    </section>
  );
}
