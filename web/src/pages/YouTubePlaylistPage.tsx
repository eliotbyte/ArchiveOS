import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import {
  ApiError,
  displayTitle,
  type CollectionDetail,
  type CollectionMemberItem,
} from "../api/client";
import PreviewImage from "../components/PreviewImage";
import { collectionTypeLabel } from "../components/youtube/YouTubeCards";
import { useVideoPlayer } from "../context/VideoPlayerContext";
import { useVaultApi } from "../context/VaultContext";
import { formatDurationLabel } from "../player/queue";

function PlaylistMemberRow({
  member,
  active,
  onSelect,
}: {
  member: CollectionMemberItem;
  active: boolean;
  onSelect: () => void;
}) {
  const title = displayTitle(member.title, undefined, member.id);
  const meta = [member.channel, member.uploader, member.status]
    .filter(Boolean)
    .join(" · ");

  return (
    <button
      type="button"
      className={`yt-playlist-row${active ? " active" : ""}`}
      onClick={onSelect}
    >
      <div className="yt-playlist-row-thumb">
        <PreviewImage preview={member.preview} title={title} compact />
      </div>
      <div className="yt-playlist-row-body">
        <div className="yt-playlist-row-title">{title}</div>
        {meta ? <div className="yt-playlist-row-meta">{meta}</div> : null}
      </div>
      {member.duration ? (
        <div className="yt-playlist-row-duration">
          {formatDurationLabel(member.duration)}
        </div>
      ) : null}
    </button>
  );
}

export default function YouTubePlaylistPage() {
  const { collectionId } = useParams();
  const api = useVaultApi();
  const { playQueue, currentItem, playEntity } = useVideoPlayer();
  const [detail, setDetail] = useState<CollectionDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!collectionId) return;
    void (async () => {
      setLoading(true);
      setError(null);
      try {
        setDetail(await api.getCollection(collectionId));
      } catch (err) {
        setError(err instanceof ApiError ? err.message : "Failed to load playlist");
        setDetail(null);
      } finally {
        setLoading(false);
      }
    })();
  }, [api, collectionId]);

  if (loading) return <div className="loading-state">Loading playlist...</div>;
  if (error) return <div className="error-state">{error}</div>;
  if (!detail) return <div className="empty-state">Playlist not found</div>;

  const playableMembers = detail.members.filter(
    (member) =>
      member.primary_asset_id && member.primary_asset_status === "present",
  );

  return (
    <section className="youtube-playlist-page">
      <Link className="back-link" to="/library/youtube">
        ← YouTube
      </Link>

      <header className="youtube-playlist-header">
        <div className="youtube-playlist-cover">
          <PreviewImage preview={detail.cover_preview} title={detail.title} compact />
        </div>
        <div className="youtube-playlist-info">
          <div className="yt-card-meta">
            {collectionTypeLabel(detail.collection_type)}
          </div>
          <h2>{detail.title}</h2>
          <p className="subtitle">{detail.member_count} videos</p>
          <div className="youtube-playlist-actions">
            <button
              className="primary"
              type="button"
              disabled={playableMembers.length === 0}
              onClick={() => playQueue(detail.members)}
            >
              Play all
            </button>
            <button
              className="secondary"
              type="button"
              disabled={playableMembers.length === 0}
              onClick={() => playQueue(detail.members, undefined, { shuffle: true })}
            >
              Shuffle
            </button>
          </div>
        </div>
      </header>

      <section className="youtube-playlist-list">
        {detail.members.map((member) => (
          <PlaylistMemberRow
            key={member.id}
            member={member}
            active={currentItem?.entityId === member.id}
            onSelect={() => {
              if (member.primary_asset_id && member.primary_asset_status === "present") {
                playQueue(detail.members, member.id);
              } else {
                playEntity({
                  id: member.id,
                  title: member.title,
                  mime: member.mime,
                  kind: member.kind,
                  status: member.status,
                  source: member.source,
                  tags: [],
                  preview: member.preview,
                  timeline_sprite: member.timeline_sprite,
                  timeline_manifest: member.timeline_manifest,
                  primary_asset_id: member.primary_asset_id,
                  primary_asset_status: member.primary_asset_status,
                });
              }
            }}
          />
        ))}
      </section>
    </section>
  );
}
