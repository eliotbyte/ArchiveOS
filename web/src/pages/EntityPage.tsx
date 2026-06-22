import { useCallback, useEffect, useState } from "react";
import { useParams } from "react-router-dom";
import {
  ApiError,
  browserDisplayState,
  displayTitle,
  isFilenameTitle,
  mediaKindFromEntity,
  type EntityDetail,
} from "../api/client";
import AssetList from "../components/AssetList";
import { useVideoPlayer } from "../context/VideoPlayerContext";
import { useVault } from "../context/VaultContext";
import { queueItemFromDetail } from "../player/queue";

export default function EntityPage() {
  const { id } = useParams();
  const { api, assetContentUrl } = useVault();
  const { playEntity, queue, registerFullPlayerAnchor, setActiveEntityAssets } =
    useVideoPlayer();
  const [entity, setEntity] = useState<EntityDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [descriptionOpen, setDescriptionOpen] = useState(false);
  const [regenerating, setRegenerating] = useState(false);

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
  }, [api, id]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (!entity || !id) return;
    setActiveEntityAssets(entity.assets);
    const primary = entity.assets.find((asset) => asset.role === "primary");
    const mediaKind = mediaKindFromEntity(primary?.kind, primary?.mime ?? entity.mime);
    if (mediaKind !== "video") return;
    if (!queueItemFromDetail(entity)) return;
    if (!queue.items.some((item) => item.entityId === id)) {
      playEntity(entity);
    }
  }, [entity, id, queue.items, playEntity, setActiveEntityAssets]);

  useEffect(() => {
    return () => registerFullPlayerAnchor(null);
  }, [id, registerFullPlayerAnchor]);

  async function regeneratePreviews() {
    if (!entity) return;
    setRegenerating(true);
    try {
      await api.regeneratePreviews(entity.id);
      await load();
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Failed to queue preview job");
    } finally {
      setRegenerating(false);
    }
  }

  if (loading) return <div className="loading-state">Loading...</div>;
  if (error) return <div className="error-state">{error}</div>;
  if (!entity) return <div className="empty-state">Entity not found</div>;

  const primary = entity.assets.find((asset) => asset.role === "primary");
  const mediaKind = mediaKindFromEntity(primary?.kind, primary?.mime ?? entity.mime);
  const title = displayTitle(entity.title, entity.metadata, entity.id);
  const showTitle = !(mediaKind === "image" && isFilenameTitle(title));
  const description =
    entity.metadata.description ??
    entity.metadata.summary ??
    entity.metadata["yt-dlp.localized_text"] ??
    "";
  const display = browserDisplayState(primary?.mime ?? entity.mime);
  const canPlay =
    primary && primary.status === "present" && display === "supported";
  const showVideoAnchor = canPlay && mediaKind === "video";

  return (
    <article className="viewer-page">
      <section
        ref={showVideoAnchor ? registerFullPlayerAnchor : undefined}
        className={`viewer-stage${showVideoAnchor ? " viewer-stage-video" : ""}`}
      >
        {showVideoAnchor ? null : canPlay && mediaKind === "image" ? (
          <img
            className="viewer-image"
            src={assetContentUrl(primary.id)}
            alt={title}
          />
        ) : canPlay && mediaKind === "audio" ? (
          <audio className="viewer-audio" controls src={assetContentUrl(primary.id)} />
        ) : primary && primary.status === "present" ? (
          <div className="viewer-fallback panel">
            <p className="media-note">
              {display === "unsupported"
                ? "This file cannot be displayed in the browser."
                : "Media is not ready to display."}
            </p>
            <a href={assetContentUrl(primary.id)} target="_blank" rel="noreferrer">
              Open original file
            </a>
          </div>
        ) : (
          <div className="viewer-fallback panel">
            <p className="media-note">
              {primary
                ? `Primary media is ${primary.status}.`
                : "No primary media on this item."}
            </p>
          </div>
        )}
      </section>

      <section className="viewer-info">
        {showTitle ? <h1 className="viewer-title">{title}</h1> : null}
        <div className="viewer-subline">
          {[entity.metadata.source, entity.metadata.channel, entity.metadata.uploader]
            .filter(Boolean)
            .join(" · ")}
        </div>

        {description ? (
          <div className={`viewer-description${descriptionOpen ? " open" : ""}`}>
            <p>{description}</p>
            {description.length > 220 ? (
              <button
                className="text-button"
                type="button"
                onClick={() => setDescriptionOpen((open) => !open)}
              >
                {descriptionOpen ? "Show less" : "Show more"}
              </button>
            ) : null}
          </div>
        ) : null}

        {entity.tags.length > 0 ? (
          <div className="badge-row viewer-tags">
            {entity.tags.map((tag) => (
              <span className="badge" key={tag}>
                {tag}
              </span>
            ))}
          </div>
        ) : null}
      </section>

      <details className="viewer-details panel">
        <summary>Technical details</summary>
        <div className="viewer-details-body">
          <button
            className="secondary"
            type="button"
            disabled={regenerating}
            onClick={() => void regeneratePreviews()}
          >
            {regenerating ? "Queueing preview job..." : "Regenerate previews"}
          </button>

          <section>
            <h3>Metadata</h3>
            <dl className="metadata-list">
              {Object.entries(entity.metadata).map(([key, value]) => (
                <div key={key}>
                  <dt>{key}</dt>
                  <dd>{value}</dd>
                </div>
              ))}
            </dl>
          </section>

          <AssetList entityId={entity.id} assets={entity.assets} onChanged={load} />
        </div>
      </details>
    </article>
  );
}
