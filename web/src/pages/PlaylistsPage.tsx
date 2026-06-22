import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { ApiError, type CollectionMemberItem, type CollectionSummary } from "../api/client";
import { useVaultApi } from "../context/VaultContext";
import PreviewImage from "../components/PreviewImage";

export default function PlaylistsPage() {
  const api = useVaultApi();
  const [collections, setCollections] = useState<CollectionSummary[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [members, setMembers] = useState<CollectionMemberItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [membersLoading, setMembersLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      setLoading(true);
      try {
        const result = await api.listCollections();
        setCollections(result);
        setSelectedId(result[0]?.id ?? null);
        setError(null);
      } catch (err) {
        setError(err instanceof ApiError ? err.message : "Failed to load playlists");
      } finally {
        setLoading(false);
      }
    })();
  }, [api]);

  useEffect(() => {
    if (!selectedId) {
      setMembers([]);
      return;
    }
    void (async () => {
      setMembersLoading(true);
      try {
        setMembers(await api.listCollectionMembers(selectedId));
      } catch (err) {
        setError(err instanceof ApiError ? err.message : "Failed to load members");
        setMembers([]);
      } finally {
        setMembersLoading(false);
      }
    })();
  }, [api, selectedId]);

  const selected = collections.find((collection) => collection.id === selectedId);

  return (
    <section className="detail-grid">
      <div className="page-header">
        <h2>Playlists</h2>
        <p className="subtitle">
          Source playlists, channels, and folder collections from your vault.
        </p>
      </div>

      {loading ? <div className="loading-state">Loading playlists...</div> : null}
      {error ? <div className="error-state">{error}</div> : null}

      {!loading && collections.length === 0 ? (
        <div className="empty-state panel">No playlists or collections yet.</div>
      ) : null}

      {collections.length > 0 ? (
        <div className="playlist-layout">
          <aside className="panel playlist-list">
            {collections.map((collection) => (
              <button
                key={collection.id}
                type="button"
                className={`playlist-item${selectedId === collection.id ? " active" : ""}`}
                onClick={() => setSelectedId(collection.id)}
              >
                <strong>{collection.title}</strong>
                <span className="card-meta">
                  {collection.collection_type} · {collection.member_count} items
                </span>
              </button>
            ))}
          </aside>

          <section className="panel">
            <h3>{selected?.title ?? "Members"}</h3>
            {membersLoading ? <div className="loading-state">Loading members...</div> : null}
            {!membersLoading && members.length === 0 ? (
              <p className="card-meta">No active members in this collection.</p>
            ) : null}
            <div className="grid playlist-members">
              {members.map((member) => (
                <Link className="card" key={member.id} to={`/entities/${member.id}`}>
                  <PreviewImage
                    preview={member.preview}
                    title={member.title}
                    compact
                  />
                  <div className="card-body">
                    <div className="card-title">{member.title ?? member.id.slice(0, 8)}</div>
                    <div className="card-meta">
                      {[member.kind, member.status].filter(Boolean).join(" · ")}
                    </div>
                  </div>
                </Link>
              ))}
            </div>
          </section>
        </div>
      ) : null}
    </section>
  );
}
