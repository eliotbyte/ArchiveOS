import type { MouseEvent } from "react";
import { Link } from "react-router-dom";
import {
  displayTitle,
  previewVisualState,
  type CollectionSummary,
  type EntityListItem,
  type LibraryListSummary,
} from "../../api/client";
import PreviewImage from "../PreviewImage";
import YouTubeEntityActions from "./YouTubeEntityActions";
import LibraryListCover from "./LibraryListCover";

function videoCardMeta(entity: EntityListItem): string | null {
  return entity.channel ?? entity.uploader ?? null;
}

function channelHref(channelEntityId: string): string {
  return `/library/youtube/channels/${channelEntityId}`;
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
  onDeleted?: (entityId: string) => void;
}

export default function YouTubeVideoCard({
  entity,
  onPlay,
  onDeleted,
}: YouTubeVideoCardProps) {
  const title = displayTitle(entity.title, undefined, entity.id);
  const meta = videoCardMeta(entity);
  const hasAvatar =
    entity.channel_entity_id != null &&
    previewVisualState(entity.channel_avatar_preview) === "ready";

  return (
    <article className="yt-card yt-video-card">
      <Link
        className="yt-card-media"
        to={`/entities/${entity.id}`}
        onClick={(event) => handlePlayClick(event, entity, onPlay)}
      >
        <PreviewImage preview={entity.preview} title={title} compact />
      </Link>
      <div className={`yt-video-card-details${hasAvatar ? " has-avatar" : ""}`}>
        {hasAvatar ? (
          <Link
            className="yt-channel-avatar-link"
            to={channelHref(entity.channel_entity_id!)}
            aria-label={meta ?? "Channel"}
          >
            <PreviewImage
              preview={entity.channel_avatar_preview}
              title={meta ?? "Channel"}
              compact
            />
          </Link>
        ) : null}
        <div className="yt-video-card-info">
          <Link
            className="yt-card-title"
            to={`/entities/${entity.id}`}
            onClick={(event) => handlePlayClick(event, entity, onPlay)}
          >
            {title}
          </Link>
          {meta ? (
            entity.channel_entity_id ? (
              <Link className="yt-card-meta" to={channelHref(entity.channel_entity_id)}>
                {meta}
              </Link>
            ) : (
              <div className="yt-card-meta">{meta}</div>
            )
          ) : null}
        </div>
        <YouTubeEntityActions
          entityId={entity.id}
          source={entity.source}
          onDeleted={() => onDeleted?.(entity.id)}
        />
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

export function libraryListTypeLabel(listType: string): string {
  switch (listType) {
    case "smart_continue_watching":
      return "Continue watching";
    case "smart_recently_added":
      return "Recently added";
    case "smart_recently_watched":
      return "Recently watched";
    case "watch_later":
      return "Watch later";
    case "playlist":
      return "Playlist";
    default:
      return listType;
  }
}

function listHref(list: LibraryListSummary): string {
  return `/library/youtube/lists/${encodeURIComponent(list.id)}`;
}

export function LibraryListCard({ list }: { list: LibraryListSummary }) {
  const typeLabel =
    list.list_kind === "source"
      ? collectionTypeLabel(list.list_type)
      : libraryListTypeLabel(list.list_type);

  return (
    <Link className="yt-card yt-playlist-card" to={listHref(list)}>
      <LibraryListCover
        preview={list.cover_preview}
        title={list.title}
        listType={list.list_type}
        memberCount={list.member_count}
        overlay={list.overlay}
        variant="card"
      />
      <div className="yt-card-body">
        <div className="yt-card-title">{list.title}</div>
        <div className="yt-card-meta">{typeLabel}</div>
      </div>
    </Link>
  );
}

/** @deprecated use LibraryListCard */
export function PlaylistCard({ collection }: PlaylistCardProps) {
  return (
    <LibraryListCard
      list={{
        id: `source:${collection.id}`,
        list_kind: "source",
        list_type: collection.collection_type,
        title: collection.title,
        member_count: collection.member_count,
        cover_preview: collection.cover_preview,
        icon: null,
        overlay: false,
      }}
    />
  );
}
