import { Link } from "react-router-dom";

export default function IntegrationsPage() {
  return (
    <section className="panel">
      <div className="page-header">
        <h2>Integrations</h2>
        <p className="subtitle">
          Optional sources and workers that enrich your library. Core browsing works
          without them.
        </p>
      </div>

      <div className="integration-grid">
        <Link className="integration-card" to="/integrations/ytdlp">
          <h3>yt-dlp</h3>
          <p className="card-meta">
            Archive or monitor URLs from supported video platforms.
          </p>
        </Link>
      </div>
    </section>
  );
}
