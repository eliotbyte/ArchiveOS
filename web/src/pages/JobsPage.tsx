import { useEffect, useState } from "react";
import { api, ApiError, type Job } from "../api/client";

export default function JobsPage() {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);

  async function loadJobs() {
    setLoading(true);
    setError(null);
    try {
      setJobs(await api.listJobs(50));
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Failed to load jobs");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void loadJobs();
  }, []);

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
        <h2>Jobs</h2>
        <button className="secondary" type="button" onClick={() => void loadJobs()}>
          Refresh
        </button>
      </div>

      {loading ? <div className="loading-state">Loading jobs...</div> : null}
      {error ? <div className="error-state">{error}</div> : null}

      <div className="job-list">
        {jobs.map((job) => (
          <article className="job-item" key={job.id}>
            <div>
              <strong>{job.type}</strong>
              <div className="job-meta">
                {job.status} · attempts {job.attempts}
              </div>
              <div className="job-meta">{job.input.slice(0, 120)}</div>
            </div>
            {job.status === "failed" || job.status === "done" ? (
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
    </section>
  );
}
