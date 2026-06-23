import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import {
  ApiError,
  type Subscription,
  type WorkerCapability,
} from "../api/client";
import ArchiveForm from "../components/ArchiveForm";
import IntegrationIcon from "../components/IntegrationIcon";
import MediaPreferencesPanel from "../components/MediaPreferencesPanel";
import MaterialIcon from "../components/youtube/MaterialIcon";
import { formatIntervalMinutes, formatRelativeTime } from "../integrations/ytdlp";
import { useVaultApi } from "../context/VaultContext";
import { useDismissOnOutside } from "../components/youtube/useDismissOnOutside";

function MonitorCard({
  sub,
  onSync,
  onRemove,
  busy,
}: {
  sub: Subscription;
  onSync: () => void;
  onRemove: () => void;
  busy: boolean;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  useDismissOnOutside(menuOpen, rootRef, () => setMenuOpen(false));

  const title = sub.collection_title ?? sub.kind;
  const playlistHref = sub.collection_id
    ? `/library/youtube/lists/source:${sub.collection_id}`
    : null;

  return (
    <article className="monitor-card job-item">
      <div className="monitor-card-main">
        <IntegrationIcon integration="ytdlp" size="sm" />
        <div>
          {playlistHref ? (
            <Link className="monitor-title" to={playlistHref}>
              {title}
            </Link>
          ) : (
            <strong className="monitor-title">{title}</strong>
          )}
          <div className="job-meta">{sub.url}</div>
          <div className="job-meta">
            <span className={`badge ${sub.status === "active" ? "ok" : "warn"}`}>
              {sub.status}
            </span>
            {" · "}
            {formatIntervalMinutes(sub.interval_minutes)}
            {" · "}
            next check {formatRelativeTime(sub.next_run_at)}
            {sub.last_checked_at
              ? ` · last ${formatRelativeTime(sub.last_checked_at)}`
              : null}
          </div>
        </div>
      </div>
      <div className="monitor-card-actions" ref={rootRef}>
        <button
          type="button"
          className="yt-icon-btn"
          aria-label="Monitor actions"
          aria-expanded={menuOpen}
          onClick={() => setMenuOpen((open) => !open)}
        >
          <MaterialIcon name="more_vert" />
        </button>
        {menuOpen ? (
          <div className="yt-entity-menu" role="menu">
            <button
              type="button"
              className="yt-entity-menu-item"
              role="menuitem"
              disabled={busy}
              onClick={() => {
                setMenuOpen(false);
                onSync();
              }}
            >
              <MaterialIcon name="sync" />
              <span>Sync now</span>
            </button>
            {playlistHref ? (
              <Link
                className="yt-entity-menu-item"
                role="menuitem"
                to={playlistHref}
                onClick={() => setMenuOpen(false)}
              >
                <MaterialIcon name="playlist_play" />
                <span>Open playlist</span>
              </Link>
            ) : null}
            <button
              type="button"
              className="yt-entity-menu-item danger-item"
              role="menuitem"
              onClick={() => {
                setMenuOpen(false);
                onRemove();
              }}
            >
              <MaterialIcon name="delete" />
              <span>Remove monitor</span>
            </button>
          </div>
        ) : null}
      </div>
    </article>
  );
}

export default function YtdlpPage() {
  const api = useVaultApi();
  const [subscriptions, setSubscriptions] = useState<Subscription[]>([]);
  const [workers, setWorkers] = useState<WorkerCapability[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);

  async function load() {
    try {
      const [subs, caps] = await Promise.all([
        api.listSubscriptions(),
        api.getCapabilities(),
      ]);
      setSubscriptions(subs);
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
    if (!window.confirm("Remove this monitor? Scheduled checks will stop.")) return;
    await api.deleteSubscription(id);
    await load();
  }

  async function syncSubscription(id: string) {
    setBusyId(id);
    try {
      await api.runSubscription(id);
      await load();
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Sync failed");
    } finally {
      setBusyId(null);
    }
  }

  const ytdlpWorker = workers.find((worker) => worker.id === "ytdlp");

  return (
    <section className="detail-grid">
      <Link className="back-link" to="/integrations">
        ← Back to integrations
      </Link>

      <div className="page-header panel integration-page-header">
        <IntegrationIcon integration="ytdlp" size="lg" />
        <div>
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
      </div>

      <ArchiveForm onSubmitted={load} />
      <MediaPreferencesPanel />

      <section className="panel">
        <div className="detail-header">
          <div>
            <h3>Monitors</h3>
            <p className="card-meta">
              Playlists and channels checked on a schedule. Use Sync now to check
              immediately.
            </p>
          </div>
          <button className="secondary" type="button" onClick={() => void load()}>
            Refresh
          </button>
        </div>
        {error ? <div className="error-state">{error}</div> : null}
        {subscriptions.length === 0 ? (
          <p className="card-meta">No active playlist/channel monitors.</p>
        ) : (
          <div className="job-list monitor-list">
            {subscriptions.map((sub) => (
              <MonitorCard
                key={sub.id}
                sub={sub}
                busy={busyId === sub.id}
                onSync={() => void syncSubscription(sub.id)}
                onRemove={() => void removeSubscription(sub.id)}
              />
            ))}
          </div>
        )}
      </section>

      <p className="card-meta">
        Import errors appear on the{" "}
        <Link to="/jobs">Jobs</Link> page under the Problems tab.
      </p>
    </section>
  );
}
