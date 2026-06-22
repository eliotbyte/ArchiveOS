import { useCallback, useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import {
  api,
  ApiError,
  assetContentUrl,
  browserDisplayState,
  type EntityDetail,
} from "../api/client";
import AssetList from "../components/AssetList";
import PreviewImage from "../components/PreviewImage";

export default function EntityPage() {
  const { id } = useParams();
  const [entity, setEntity] = useState<EntityDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    if (!id) return;
    setLoading(true);
    setError(null);
    try {
      setEntity(await api.getEntity(id));
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Failed to load entity");
      setEntity(null);
    } finally {
      setLoading(false);
    }
  }, [id]);

  useEffect(() => {
    void load();
  }, [load]);

  if (loading) return <div className="loading-state">Loading entity...</div>;
  if (error) return <div className="error-state">{error}</div>;
  if (!entity) return <div className="empty-state">Entity not found</div>;

  const title =
    entity.metadata.title ?? entity.metadata.name ?? entity.id.slice(0, 8);
  const primary = entity.assets.find((asset) => asset.role === "primary");
  const display = browserDisplayState(primary?.mime ?? entity.mime);

  return (
    <section className="detail-grid">
      <Link className="back-link" to="/">
        ← Back to explore
      </Link>

      <div className="detail-header panel">
        <div>
          <h2>{title}</h2>
          <p className="subtitle">
            {[entity.status, entity.mime, entity.metadata.source]
              .filter(Boolean)
              .join(" · ")}
          </p>
          <div className="badge-row">
            {entity.tags.map((tag) => (
              <span className="badge" key={tag}>
                {tag}
              </span>
            ))}
          </div>
        </div>
        <PreviewImage preview={entity.preview} title={title} />
      </div>

      <section className="panel media-panel">
        <h3>Browser media</h3>
        {primary && primary.status === "present" && display === "supported" ? (
          <>
            {primary.mime?.startsWith("video/") ? (
              <video controls src={assetContentUrl(primary.id)} />
            ) : primary.mime?.startsWith("audio/") ? (
              <audio controls src={assetContentUrl(primary.id)} />
            ) : primary.mime?.startsWith("image/") ? (
              <img src={assetContentUrl(primary.id)} alt={title} />
            ) : (
              <a href={assetContentUrl(primary.id)} target="_blank" rel="noreferrer">
                Open in browser
              </a>
            )}
          </>
        ) : (
          <p className="media-note">
            {primary
              ? display === "unsupported"
                ? "Primary media exists but cannot be displayed in the browser."
                : `Primary media is ${primary.status}.`
              : "No primary asset on this entity."}
          </p>
        )}
      </section>

      <AssetList entityId={entity.id} assets={entity.assets} onChanged={load} />

      <section className="panel">
        <h3>Metadata</h3>
        <dl>
          {Object.entries(entity.metadata).map(([key, value]) => (
            <div key={key}>
              <dt>{key}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
      </section>
    </section>
  );
}
