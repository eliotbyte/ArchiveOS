import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import {
  displayTitle,
  isFilenameTitle,
  kindIcon,
  mediaKindFromEntity,
  type EntityListItem,
  type TimelineManifest,
} from "../api/client";
import { useVault } from "../context/VaultContext";
import PreviewImage from "./PreviewImage";

interface EntityCardProps {
  entity: EntityListItem;
}

export default function EntityCard({ entity }: EntityCardProps) {
  const { assetContentUrl } = useVault();
  const mediaKind = mediaKindFromEntity(entity.kind, entity.mime);
  const title = displayTitle(entity.title, undefined, entity.id);
  const showTitle = !isFilenameTitle(title) || mediaKind === "video";
  const isVideo = mediaKind === "video";
  const canScrub =
    isVideo &&
    entity.timeline_sprite?.status === "present" &&
    entity.timeline_manifest?.status === "present";
  const [manifest, setManifest] = useState<TimelineManifest | null>(null);
  const [hoverX, setHoverX] = useState<number | null>(null);
  const [scrubbing, setScrubbing] = useState(false);

  useEffect(() => {
    if (!canScrub || !entity.timeline_manifest?.asset_id) {
      setManifest(null);
      return;
    }

    void (async () => {
      try {
        const response = await fetch(
          assetContentUrl(entity.timeline_manifest!.asset_id),
        );
        if (!response.ok) return;
        setManifest((await response.json()) as TimelineManifest);
      } catch {
        setManifest(null);
      }
    })();
  }, [assetContentUrl, canScrub, entity.timeline_manifest]);

  const frameIndex =
    hoverX !== null && manifest && manifest.frames.length > 0
      ? Math.min(
          manifest.frames.length - 1,
          Math.max(0, Math.floor(hoverX * manifest.frames.length)),
        )
      : null;

  const spriteStyle =
    frameIndex !== null && manifest && entity.timeline_sprite?.asset_id
      ? {
          backgroundImage: `url(${assetContentUrl(entity.timeline_sprite.asset_id)})`,
          backgroundSize: `${manifest.columns * 100}% ${manifest.rows * 100}%`,
          backgroundPosition: `${
            (frameIndex % manifest.columns) * (100 / (manifest.columns - 1 || 1))
          }% ${
            Math.floor(frameIndex / manifest.columns) *
            (100 / (manifest.rows - 1 || 1))
          }%`,
        }
      : undefined;

  const showFooter = isVideo && (showTitle || Boolean(entity.source));

  return (
    <Link
      className={`card kind-${mediaKind}`}
      to={`/entities/${entity.id}`}
      onMouseLeave={() => {
        setHoverX(null);
        setScrubbing(false);
      }}
    >
      <div
        className={`preview-shell${scrubbing ? " scrubbing" : ""}`}
        onMouseMove={(event) => {
          if (!canScrub || !manifest) return;
          const rect = event.currentTarget.getBoundingClientRect();
          const x = (event.clientX - rect.left) / rect.width;
          setHoverX(Math.min(1, Math.max(0, x)));
          setScrubbing(true);
        }}
      >
        {scrubbing && spriteStyle ? (
          <div className="preview-frame scrub-frame" style={spriteStyle} />
        ) : (
          <PreviewImage preview={entity.preview} title={title} compact />
        )}
        {isVideo ? (
          <span className="kind-badge" title="video">
            {kindIcon("video")}
          </span>
        ) : null}
      </div>
      {showFooter ? (
        <div className="card-body">
          {showTitle ? <div className="card-title">{title}</div> : null}
          {entity.source ? (
            <div className="card-meta">{entity.source}</div>
          ) : null}
        </div>
      ) : null}
    </Link>
  );
}
