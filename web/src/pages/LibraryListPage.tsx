import { useEffect, useRef, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router-dom";
import {
  ApiError,
  BROWSE_SORT_LABELS,
  type BrowseSort,
  displayTitle,
  type CollectionMemberItem,
  type LibraryListDetail,
} from "../api/client";
import ConfirmDialog from "../components/ConfirmDialog";
import IntegrationIcon from "../components/IntegrationIcon";
import PreviewImage from "../components/PreviewImage";
import LibraryListCover from "../components/youtube/LibraryListCover";
import MaterialIcon from "../components/youtube/MaterialIcon";
import YouTubeEntityActions from "../components/youtube/YouTubeEntityActions";
import {
  collectionTypeLabel,
  libraryListTypeLabel,
} from "../components/youtube/YouTubeCards";
import { useDismissOnOutside } from "../components/youtube/useDismissOnOutside";
import { useVideoPlayer } from "../context/VideoPlayerContext";
import { useVaultApi } from "../context/VaultContext";
import { formatDurationLabel } from "../player/queue";

function ListMemberRow({
  member,
  active,
  onSelect,
  onRemove,
  canRemove,
  onActionError,
  onDeleted,
}: {
  member: CollectionMemberItem;
  active: boolean;
  onSelect: () => void;
  onRemove: () => void;
  canRemove: boolean;
  onActionError: (message: string) => void;
  onDeleted: () => void;
}) {
  const title = displayTitle(member.title, undefined, member.id);
  const meta = [member.channel, member.uploader, member.status]
    .filter(Boolean)
    .join(" · ");
  const progress =
    member.playback_progress != null
      ? Math.round(member.playback_progress * 100)
      : null;

  return (
    <div className={`yt-playlist-row-wrap${active ? " active" : ""}`}>
      <div className="yt-playlist-row">
        <button type="button" className="yt-playlist-row-main" onClick={onSelect}>
          <div className="yt-playlist-row-thumb">
            <PreviewImage preview={member.preview} title={title} compact />
            {progress != null && progress > 0 && progress < 95 ? (
              <div
                className="yt-playlist-row-progress"
                style={{ width: `${progress}%` }}
              />
            ) : null}
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
        <YouTubeEntityActions
          entityId={member.id}
          source={member.source}
          canRemove={canRemove}
          onRemove={onRemove}
          onDeleted={onDeleted}
          onActionError={onActionError}
        />
      </div>
    </div>
  );
}

function SourceListMenu({
  onSync,
  onDelete,
}: {
  onSync: () => void;
  onDelete: () => void;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  useDismissOnOutside(open, rootRef, () => setOpen(false));

  return (
    <div className="yt-entity-actions" ref={rootRef}>
      <button
        type="button"
        className="yt-icon-btn"
        aria-label="Playlist actions"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <MaterialIcon name="more_vert" />
      </button>
      {open ? (
        <div className="yt-entity-menu" role="menu">
          <button
            type="button"
            className="yt-entity-menu-item"
            role="menuitem"
            onClick={() => {
              setOpen(false);
              onSync();
            }}
          >
            <MaterialIcon name="sync" />
            <span>Sync from yt-dlp</span>
          </button>
          <button
            type="button"
            className="yt-entity-menu-item danger-item"
            role="menuitem"
            onClick={() => {
              setOpen(false);
              onDelete();
            }}
          >
            <MaterialIcon name="delete" />
            <span>Delete playlist</span>
          </button>
        </div>
      ) : null}
    </div>
  );
}

export default function LibraryListPage() {
  const params = useParams();
  const navigate = useNavigate();
  const rawId = params.listId ?? params.collectionId ?? "";
  const decodedListId = decodeURIComponent(rawId);
  const resolvedListId =
    decodedListId.includes(":") ? decodedListId : `source:${decodedListId}`;
  const [searchParams, setSearchParams] = useSearchParams();
  const sort = (searchParams.get("sort") ?? undefined) as BrowseSort | undefined;
  const api = useVaultApi();
  const { playQueue, currentItem, playEntity } = useVideoPlayer();
  const [detail, setDetail] = useState<LibraryListDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [confirmDeletePlaylist, setConfirmDeletePlaylist] = useState(false);

  const collectionId =
    detail?.list_kind === "source" && detail.id.startsWith("source:")
      ? detail.id.slice("source:".length)
      : null;

  useEffect(() => {
    if (!resolvedListId) return;
    void (async () => {
      setLoading(true);
      setError(null);
      try {
        setDetail(
          await api.getLibraryList(resolvedListId, {
            sort,
            section: "youtube",
          }),
        );
      } catch (err) {
        setError(err instanceof ApiError ? err.message : "Failed to load list");
        setDetail(null);
      } finally {
        setLoading(false);
      }
    })();
  }, [api, resolvedListId, sort]);

  async function handleRemove(entityId: string) {
    if (!detail) return;
    setActionError(null);
    try {
      if (detail.list_kind === "user" && detail.id.startsWith("user:")) {
        const userListId = detail.id.slice("user:".length);
        await api.removeUserListMember(userListId, entityId);
      } else if (collectionId) {
        await api.removeCollectionMember(collectionId, entityId);
      } else {
        return;
      }
      setDetail({
        ...detail,
        members: detail.members.filter((member) => member.id !== entityId),
        member_count: detail.member_count - 1,
      });
    } catch (err) {
      setActionError(
        err instanceof ApiError ? err.message : "Failed to remove from list",
      );
    }
  }

  async function handleSyncPlaylist() {
    if (!collectionId) return;
    setActionError(null);
    try {
      await api.refreshCollectionFromSource(collectionId);
    } catch (err) {
      setActionError(
        err instanceof ApiError ? err.message : "Failed to queue sync",
      );
    }
  }

  async function handleDeletePlaylist() {
    if (!collectionId) return;
    setConfirmDeletePlaylist(false);
    setActionError(null);
    try {
      await api.deleteEntity(collectionId);
      navigate("/library/youtube");
    } catch (err) {
      setActionError(
        err instanceof ApiError ? err.message : "Failed to delete playlist",
      );
    }
  }

  function handleMemberDeleted(entityId: string) {
    if (!detail) return;
    setDetail({
      ...detail,
      members: detail.members.filter((member) => member.id !== entityId),
      member_count: detail.member_count - 1,
    });
  }

  if (loading) return <div className="loading-state">Loading list...</div>;
  if (error) return <div className="error-state">{error}</div>;
  if (!detail) return <div className="empty-state">List not found</div>;

  const playableMembers = detail.members.filter(
    (member) =>
      member.primary_asset_id && member.primary_asset_status === "present",
  );
  const typeLabel =
    detail.list_kind === "source"
      ? collectionTypeLabel(detail.list_type)
      : libraryListTypeLabel(detail.list_type);
  const canRemove = detail.list_kind === "user" || detail.list_kind === "source";
  const isSourceList = detail.list_kind === "source";

  return (
    <section className="youtube-playlist-page">
      <Link className="back-link" to="/library/youtube">
        ← YouTube
      </Link>

      <header className="youtube-playlist-header">
        <LibraryListCover
          preview={detail.cover_preview}
          title={detail.title}
          listType={detail.list_type}
          memberCount={detail.member_count}
          overlay={detail.overlay}
          variant="header"
        />
        <div className="youtube-playlist-info">
          <div className="playlist-header-meta">
            <div className="yt-card-meta">{typeLabel}</div>
            {isSourceList ? (
              <span className="source-badge">
                <IntegrationIcon integration="ytdlp" size="sm" />
                yt-dlp
              </span>
            ) : null}
          </div>
          <h2>{detail.title}</h2>
          <p className="subtitle">{detail.member_count} items</p>
          {detail.sort_options.length > 1 ? (
            <label className="yt-sort-control">
              Sort
              <select
                value={sort ?? detail.default_sort}
                onChange={(event) => {
                  const next = new URLSearchParams(searchParams);
                  next.set("sort", event.target.value);
                  setSearchParams(next);
                }}
              >
                {detail.sort_options.map((option) => (
                  <option key={option} value={option}>
                    {BROWSE_SORT_LABELS[option as BrowseSort] ?? option}
                  </option>
                ))}
              </select>
            </label>
          ) : null}
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
              onClick={() =>
                playQueue(detail.members, undefined, { shuffle: true })
              }
            >
              Shuffle
            </button>
            {isSourceList ? (
              <SourceListMenu
                onSync={() => void handleSyncPlaylist()}
                onDelete={() => setConfirmDeletePlaylist(true)}
              />
            ) : null}
          </div>
        </div>
      </header>

      {actionError ? <div className="error-state">{actionError}</div> : null}

      <section className="youtube-playlist-list">
        {detail.members.map((member) => (
          <ListMemberRow
            key={member.id}
            member={member}
            active={currentItem?.entityId === member.id}
            canRemove={canRemove}
            onActionError={setActionError}
            onRemove={() => void handleRemove(member.id)}
            onDeleted={() => handleMemberDeleted(member.id)}
            onSelect={() => {
              if (
                member.primary_asset_id &&
                member.primary_asset_status === "present"
              ) {
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

      <ConfirmDialog
        open={confirmDeletePlaylist}
        title="Delete playlist?"
        message="The playlist and its local archive will be removed. Individual videos stay archived unless you delete them separately."
        confirmLabel="Delete playlist"
        destructive
        onCancel={() => setConfirmDeletePlaylist(false)}
        onConfirm={() => void handleDeletePlaylist()}
      />
    </section>
  );
}
