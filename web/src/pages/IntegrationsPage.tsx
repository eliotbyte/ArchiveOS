import { Link } from "react-router-dom";
import IntegrationIcon from "../components/IntegrationIcon";

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
          <IntegrationIcon integration="ytdlp" size="lg" />
          <div className="integration-card-body">
            <h3>yt-dlp</h3>
            <p className="card-meta">
              Archive or monitor URLs from supported video platforms.
            </p>
          </div>
        </Link>
      </div>
    </section>
  );
}
