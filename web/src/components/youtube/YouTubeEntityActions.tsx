import { useRef, useState } from "react";
import { ApiError } from "../../api/client";
import { isYtdlpSource } from "../../integrations/ytdlp";
import { useVaultApi } from "../../context/VaultContext";
import ConfirmDialog from "../ConfirmDialog";
import MaterialIcon from "./MaterialIcon";
import SaveToPlaylistPopover from "./SaveToPlaylistPopover";
import { useDismissOnOutside } from "./useDismissOnOutside";

interface YouTubeEntityActionsProps {
  entityId: string;
  source?: string | null;
  canRemove?: boolean;
  onRemove?: () => void;
  onDeleted?: () => void;
  onActionError?: (message: string) => void;
}

export default function YouTubeEntityActions({
  entityId,
  source,
  canRemove = false,
  onRemove,
  onDeleted,
  onActionError,
}: YouTubeEntityActionsProps) {
  const api = useVaultApi();
  const rootRef = useRef<HTMLDivElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const [saveToOpen, setSaveToOpen] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [busy, setBusy] = useState(false);

  const fromYtdlp = isYtdlpSource(source);

  useDismissOnOutside(menuOpen || saveToOpen, rootRef, () => {
    setMenuOpen(false);
    setSaveToOpen(false);
  });

  async function handleWatchLater() {
    setMenuOpen(false);
    try {
      await api.addToWatchLater(entityId);
    } catch (err) {
      onActionError?.(
        err instanceof ApiError ? err.message : "Failed to add to Watch Later",
      );
    }
  }

  async function handleRefreshMetadata() {
    setMenuOpen(false);
    setBusy(true);
    try {
      await api.refreshEntityFromSource(entityId, { metadata_only: true });
    } catch (err) {
      onActionError?.(
        err instanceof ApiError ? err.message : "Failed to refresh metadata",
      );
    } finally {
      setBusy(false);
    }
  }

  async function handleDelete() {
    setConfirmDelete(false);
    setBusy(true);
    try {
      await api.deleteEntity(entityId);
      onDeleted?.();
    } catch (err) {
      onActionError?.(
        err instanceof ApiError ? err.message : "Failed to delete video",
      );
    } finally {
      setBusy(false);
    }
  }

  function handleRemove() {
    setMenuOpen(false);
    onRemove?.();
  }

  return (
    <div className="yt-entity-actions" ref={rootRef}>
      <button
        type="button"
        className="yt-icon-btn yt-entity-menu-btn"
        aria-label="More actions"
        aria-expanded={menuOpen || saveToOpen}
        disabled={busy}
        onClick={(event) => {
          event.stopPropagation();
          setSaveToOpen(false);
          setMenuOpen((open) => !open);
        }}
      >
        <MaterialIcon name="more_vert" />
      </button>

      {menuOpen ? (
        <div className="yt-entity-menu" role="menu">
          <button
            type="button"
            className="yt-entity-menu-item"
            role="menuitem"
            onClick={() => void handleWatchLater()}
          >
            <MaterialIcon name="schedule" />
            <span>Save to Watch Later</span>
          </button>
          <button
            type="button"
            className="yt-entity-menu-item"
            role="menuitem"
            onClick={() => {
              setMenuOpen(false);
              setSaveToOpen(true);
            }}
          >
            <MaterialIcon name="playlist_add" />
            <span>Save to playlist...</span>
          </button>
          {fromYtdlp ? (
            <button
              type="button"
              className="yt-entity-menu-item"
              role="menuitem"
              onClick={() => void handleRefreshMetadata()}
            >
              <MaterialIcon name="sync" />
              <span>Refresh metadata (yt-dlp)</span>
            </button>
          ) : null}
          {canRemove ? (
            <button
              type="button"
              className="yt-entity-menu-item"
              role="menuitem"
              onClick={handleRemove}
            >
              <MaterialIcon name="playlist_remove" />
              <span>Remove from playlist</span>
            </button>
          ) : null}
          <button
            type="button"
            className="yt-entity-menu-item danger-item"
            role="menuitem"
            onClick={() => {
              setMenuOpen(false);
              setConfirmDelete(true);
            }}
          >
            <MaterialIcon name="delete" />
            <span>Delete video</span>
          </button>
        </div>
      ) : null}

      {saveToOpen ? (
        <SaveToPlaylistPopover
          entityId={entityId}
          onClose={() => setSaveToOpen(false)}
        />
      ) : null}

      <ConfirmDialog
        open={confirmDelete}
        title="Delete video?"
        message="The video file and metadata will be removed from ArchiveOS. yt-dlp will not download it again."
        confirmLabel="Delete"
        destructive
        onCancel={() => setConfirmDelete(false)}
        onConfirm={() => void handleDelete()}
      />
    </div>
  );
}
