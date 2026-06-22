import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import {
  ApiError,
  type SourceFailure,
  type Subscription,
  type WorkerCapability,
} from "../api/client";
import { useVaultApi } from "../context/VaultContext";
import ArchiveForm from "../components/ArchiveForm";

export default function YtdlpPage() {
  const api = useVaultApi();
  const [subscriptions, setSubscriptions] = useState<Subscription[]>([]);
  const [failures, setFailures] = useState<SourceFailure[]>([]);
  const [workers, setWorkers] = useState<WorkerCapability[]>([]);
  const [error, setError] = useState<string | null>(null);

  async function load() {
    try {
      const [subs, fails, caps] = await Promise.all([
        api.listSubscriptions(),
        api.listSourceFailures(),
        api.getCapabilities(),
      ]);
      setSubscriptions(subs);
      setFailures(fails.slice(0, 20));
      setWorkers(caps.workers);
      setError(null);
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Failed to load integration data");
    }
  }

  useEffect(() => {
    void load();
  }, [api]);

  async function removeSubscription(id: string) {
    await api.deleteSubscription(id);
    await load();
  }

  const ytdlpWorker = workers.find((worker) => worker.id === "ytdlp");

  return (
    <section className="detail-grid">
      <Link className="back-link" to="/integrations">
        ← Back to integrations
      </Link>

      <div className="page-header panel">
        <h2>yt-dlp</h2>
        <p className="subtitle">
          Archive URLs or subscribe to playlists/channels for periodic sync.
        </p>
        {ytdlpWorker ? (
          <span className={`badge ${ytdlpWorker.enabled ? "ok" : "warn"}`}>
            worker: {ytdlpWorker.enabled ? "available" : "not configured"}
          </span>
        ) : null}
      </div>

      <ArchiveForm onSubmitted={load} />

      <section className="panel">
        <div className="detail-header">
          <h3>Monitors</h3>
          <button className="secondary" type="button" onClick={() => void load()}>
            Refresh
          </button>
        </div>
        {error ? <div className="error-state">{error}</div> : null}
        {subscriptions.length === 0 ? (
          <p className="card-meta">No active playlist/channel monitors.</p>
        ) : (
          <div className="job-list">
            {subscriptions.map((sub) => (
              <article className="job-item" key={sub.id}>
                <div>
                  <strong>{sub.kind}</strong>
                  <div className="job-meta">{sub.url}</div>
                  <div className="job-meta">
                    {sub.status} · every {sub.interval_minutes}m · next {sub.next_run_at}
                  </div>
                </div>
                <button
                  className="secondary"
                  type="button"
                  onClick={() => void removeSubscription(sub.id)}
                >
                  Remove
                </button>
              </article>
            ))}
          </div>
        )}
      </section>

      <section className="panel">
        <h3>Recent failures</h3>
        {failures.length === 0 ? (
          <p className="card-meta">No recorded source failures.</p>
        ) : (
          <div className="job-list">
            {failures.map((failure) => (
              <article className="job-item" key={failure.id}>
                <div>
                  <strong>
                    {failure.source}/{failure.kind}
                  </strong>
                  <div className="job-meta">{failure.message}</div>
                  <div className="job-meta">
                    {failure.stage} · {failure.error_kind} · {failure.created_at}
                  </div>
                </div>
              </article>
            ))}
          </div>
        )}
      </section>
    </section>
  );
}
