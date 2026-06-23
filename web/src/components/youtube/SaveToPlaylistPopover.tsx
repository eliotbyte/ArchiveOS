import { useEffect, useRef, useState } from "react";
import { ApiError, type LibraryListSummary } from "../../api/client";
import { useVaultApi } from "../../context/VaultContext";
import MaterialIcon from "./MaterialIcon";
import { useDismissOnOutside } from "./useDismissOnOutside";

interface SaveToPlaylistPopoverProps {
  entityId: string;
  onClose: () => void;
}

function userPlaylistId(list: LibraryListSummary): string | null {
  if (!list.id.startsWith("user:")) return null;
  return list.id.slice("user:".length);
}

export default function SaveToPlaylistPopover({
  entityId,
  onClose,
}: SaveToPlaylistPopoverProps) {
  const api = useVaultApi();
  const rootRef = useRef<HTMLDivElement>(null);
  const [playlists, setPlaylists] = useState<LibraryListSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [searchOpen, setSearchOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [creating, setCreating] = useState(false);
  const [newTitle, setNewTitle] = useState("");
  const [saving, setSaving] = useState(false);

  useDismissOnOutside(true, rootRef, onClose);

  useEffect(() => {
    let cancelled = false;

    void (async () => {
      setLoading(true);
      setError(null);
      try {
        const lists = await api.listLibraryLists({ section: "youtube" });
        if (cancelled) return;
        setPlaylists(
          lists.filter(
            (list) =>
              list.list_kind === "user" && list.list_type === "playlist",
          ),
        );
      } catch (err) {
        if (cancelled) return;
        setError(
          err instanceof ApiError ? err.message : "Failed to load playlists",
        );
        setPlaylists([]);
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
    // Load once when the popover opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const normalizedQuery = query.trim().toLowerCase();
  const filtered = normalizedQuery
    ? playlists.filter((list) =>
        list.title.toLowerCase().includes(normalizedQuery),
      )
    : playlists;

  async function saveToList(list: LibraryListSummary) {
    const listId = userPlaylistId(list);
    if (!listId) return;
    setSaving(true);
    setError(null);
    try {
      await api.addUserListMember(listId, entityId);
      onClose();
    } catch (err) {
      setError(
        err instanceof ApiError ? err.message : "Failed to save to playlist",
      );
    } finally {
      setSaving(false);
    }
  }

  async function handleCreatePlaylist() {
    const title = newTitle.trim();
    if (!title) return;
    setSaving(true);
    setError(null);
    try {
      const { id } = await api.createUserList({
        title,
        list_type: "playlist",
      });
      await api.addUserListMember(id, entityId);
      onClose();
    } catch (err) {
      setError(
        err instanceof ApiError ? err.message : "Failed to create playlist",
      );
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="yt-save-to-popover" ref={rootRef} role="dialog" aria-label="Save to playlist">
      <div className="yt-save-to-header">
        <span className="yt-save-to-title">Save to</span>
        <button
          type="button"
          className="yt-icon-btn"
          aria-label={searchOpen ? "Close search" : "Search playlists"}
          onClick={() => {
            setSearchOpen((open) => !open);
            if (searchOpen) setQuery("");
          }}
        >
          <MaterialIcon name={searchOpen ? "close" : "search"} />
        </button>
      </div>

      {searchOpen ? (
        <input
          className="yt-save-to-search"
          type="search"
          placeholder="Search playlists"
          value={query}
          autoFocus
          onChange={(event) => setQuery(event.target.value)}
        />
      ) : null}

      {error ? <div className="yt-save-to-error">{error}</div> : null}

      <div className="yt-save-to-list" role="listbox">
        {loading ? (
          <div className="yt-save-to-empty">Loading playlists...</div>
        ) : filtered.length === 0 ? (
          <div className="yt-save-to-empty">
            {normalizedQuery ? "No playlists match your search" : "No playlists yet"}
          </div>
        ) : (
          filtered.map((list) => (
            <button
              key={list.id}
              type="button"
              className="yt-save-to-item"
              role="option"
              aria-label={list.title}
              disabled={saving}
              onClick={() => void saveToList(list)}
            >
              <MaterialIcon name="playlist_play" />
              <span>{list.title}</span>
            </button>
          ))
        )}
      </div>

      {creating ? (
        <form
          className="yt-save-to-create-form"
          onSubmit={(event) => {
            event.preventDefault();
            void handleCreatePlaylist();
          }}
        >
          <input
            type="text"
            placeholder="Playlist title"
            value={newTitle}
            autoFocus
            onChange={(event) => setNewTitle(event.target.value)}
          />
          <button type="submit" className="primary" disabled={saving || !newTitle.trim()}>
            Create
          </button>
          <button
            type="button"
            className="yt-icon-btn"
            aria-label="Cancel"
            onClick={() => {
              setCreating(false);
              setNewTitle("");
            }}
          >
            <MaterialIcon name="close" />
          </button>
        </form>
      ) : (
        <button
          type="button"
          className="yt-save-to-create"
          disabled={saving}
          onClick={() => setCreating(true)}
        >
          <MaterialIcon name="add" />
          <span>Create new playlist</span>
        </button>
      )}
    </div>
  );
}
