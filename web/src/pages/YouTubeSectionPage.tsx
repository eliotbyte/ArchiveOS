import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { ApiError, type CollectionSummary, type EntityListItem } from "../api/client";
import YouTubeVideoCard, { PlaylistCard } from "../components/youtube/YouTubeCards";
import { useVideoPlayer } from "../context/VideoPlayerContext";
import { useVaultApi } from "../context/VaultContext";
import {
  getSectionById,
  isYouTubeCollection,
  sectionBrowseParams,
} from "../library/sections";

export default function YouTubeSectionPage() {
  const section = getSectionById("youtube")!;
  const api = useVaultApi();
  const { playEntity } = useVideoPlayer();
  const [videos, setVideos] = useState<EntityListItem[]>([]);
  const [collections, setCollections] = useState<CollectionSummary[]>([]);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      setLoading(true);
      setError(null);
      try {
        const [videoItems, allCollections] = await Promise.all([
          api.listEntities(sectionBrowseParams(section, query.trim())),
          api.listCollections({ min_member_count: 1 }),
        ]);
        setVideos(videoItems);
        setCollections(
          allCollections.filter(
            (collection) =>
              isYouTubeCollection(collection.collection_type) &&
              collection.member_count > 0,
          ),
        );
      } catch (err) {
        setError(err instanceof ApiError ? err.message : "Failed to load YouTube section");
        setVideos([]);
        setCollections([]);
      } finally {
        setLoading(false);
      }
    })();
  }, [api, query, section]);

  return (
    <section className="youtube-page">
      <div className="page-header">
        <h2>{section.label}</h2>
        <p className="subtitle">{section.description}</p>
      </div>

      <form
        className="toolbar youtube-toolbar"
        onSubmit={(event) => {
          event.preventDefault();
        }}
      >
        <input
          placeholder="Search YouTube videos"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        <button
          className="secondary"
          type="button"
          onClick={() => setQuery("")}
        >
          Reset
        </button>
      </form>

      {loading ? <div className="loading-state">Loading YouTube...</div> : null}
      {error ? <div className="error-state">{error}</div> : null}

      {!loading && collections.length > 0 ? (
        <section className="youtube-shelf">
          <h3>Playlists &amp; channels</h3>
          <div className="youtube-playlist-grid">
            {collections.map((collection) => (
              <PlaylistCard key={collection.id} collection={collection} />
            ))}
          </div>
        </section>
      ) : null}

      {!loading ? (
        <section className="youtube-shelf">
          <h3>Videos</h3>
          {videos.length === 0 ? (
            <div className="empty-state">
              No YouTube videos archived yet. Use{" "}
              <Link to="/integrations">Integrations</Link> to archive URLs.
            </div>
          ) : (
            <div className="youtube-video-grid">
              {videos.map((entity) => (
                <YouTubeVideoCard
                  key={entity.id}
                  entity={entity}
                  onPlay={playEntity}
                />
              ))}
            </div>
          )}
        </section>
      ) : null}
    </section>
  );
}
