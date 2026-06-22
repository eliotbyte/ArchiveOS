import type { MouseEvent } from "react";
import { Link } from "react-router-dom";
import {
  displayTitle,
  type CollectionSummary,
  type EntityListItem,
} from "../../api/client";
import PreviewImage from "../PreviewImage";

function videoCardMeta(entity: EntityListItem): string | null {
  return entity.channel ?? entity.uploader ?? null;
}

function handlePlayClick(
  event: MouseEvent<HTMLAnchorElement>,
  entity: EntityListItem,
  onPlay?: (entity: EntityListItem) => void,
) {
  if (
    event.button !== 0 ||
    event.metaKey ||
    event.ctrlKey ||
    event.shiftKey ||
    event.altKey
  ) {
    return;
  }
  event.preventDefault();
  onPlay?.(entity);
}

interface YouTubeVideoCardProps {
  entity: EntityListItem;
  onPlay?: (entity: EntityListItem) => void;
}

export default function YouTubeVideoCard({
  entity,
  onPlay,
}: YouTubeVideoCardProps) {
  const title = displayTitle(entity.title, undefined, entity.id);
  const meta = videoCardMeta(entity);

  return (
    <article className="yt-card yt-video-card">
      <Link
        className="yt-card-media"
        to={`/entities/${entity.id}`}
        onClick={(event) => handlePlayClick(event, entity, onPlay)}
      >
        <PreviewImage preview={entity.preview} title={title} compact />
      </Link>
      <div className="yt-card-body">
        <Link
          className="yt-card-title"
          to={`/entities/${entity.id}`}
          onClick={(event) => handlePlayClick(event, entity, onPlay)}
        >
          {title}
        </Link>
        {meta ? <div className="yt-card-meta">{meta}</div> : null}
      </div>
    </article>
  );
}

interface PlaylistCardProps {
  collection: CollectionSummary;
}

export function collectionTypeLabel(collectionType: string): string {
  switch (collectionType) {
    case "youtube_playlist":
      return "Playlist";
    case "youtube_channel_uploads":
      return "Channel uploads";
    default:
      return collectionType;
  }
}

export function PlaylistCard({ collection }: PlaylistCardProps) {
  return (
    <Link
      className="yt-card yt-playlist-card"
      to={`/library/youtube/playlists/${collection.id}`}
    >
      <div className="yt-playlist-stack" aria-hidden="true">
        <div className="yt-playlist-layer yt-playlist-layer-back" />
        <div className="yt-playlist-layer yt-playlist-layer-mid" />
        <div className="yt-playlist-cover">
          <PreviewImage
            preview={collection.cover_preview}
            title={collection.title}
            compact
          />
        </div>
      </div>
      <div className="yt-card-body">
        <div className="yt-card-title">{collection.title}</div>
        <div className="yt-card-meta">
          {collectionTypeLabel(collection.collection_type)} ·{" "}
          {collection.member_count} videos
        </div>
      </div>
    </Link>
  );
}
