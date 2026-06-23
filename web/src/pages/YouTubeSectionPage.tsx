import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { ApiError, BROWSE_SORT_LABELS, type BrowseSort, type EntityListItem, type LibraryListSummary } from "../api/client";
import YouTubeVideoCard, { LibraryListCard } from "../components/youtube/YouTubeCards";
import { useVideoPlayer } from "../context/VideoPlayerContext";
import { useVaultApi } from "../context/VaultContext";
import { getSectionById, sectionBrowseParams } from "../library/sections";

export default function YouTubeSectionPage() {
  const section = getSectionById("youtube")!;
  const api = useVaultApi();
  const { playEntity } = useVideoPlayer();
  const [videos, setVideos] = useState<EntityListItem[]>([]);
  const [libraryLists, setLibraryLists] = useState<LibraryListSummary[]>([]);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<BrowseSort>("added_desc");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      setLoading(true);
      setError(null);
      try {
        const [videoItems, lists] = await Promise.all([
          api.listEntities({
            ...sectionBrowseParams(section, query.trim()),
            sort,
          }),
          api.listLibraryLists({ section: "youtube" }),
        ]);
        setVideos(videoItems);
        setLibraryLists(lists);
      } catch (err) {
        setError(err instanceof ApiError ? err.message : "Failed to load YouTube section");
        setVideos([]);
        setLibraryLists([]);
      } finally {
        setLoading(false);
      }
    })();
  }, [api, query, section, sort]);

  const smartLists = libraryLists.filter((list) => list.list_kind === "smart");
  const userLists = libraryLists.filter((list) => list.list_kind === "user");
  const sourceLists = libraryLists.filter((list) => list.list_kind === "source");

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
        <label className="yt-sort-control inline">
          Sort
          <select
            value={sort}
            onChange={(event) => setSort(event.target.value as BrowseSort)}
          >
            <option value="added_desc">{BROWSE_SORT_LABELS.added_desc}</option>
            <option value="published_desc">{BROWSE_SORT_LABELS.published_desc}</option>
            <option value="views_desc">{BROWSE_SORT_LABELS.views_desc}</option>
          </select>
        </label>
        <button
          className="secondary"
          type="button"
          onClick={() => {
            setQuery("");
            setSort("added_desc");
          }}
        >
          Reset
        </button>
      </form>

      {loading ? <div className="loading-state">Loading YouTube...</div> : null}
      {error ? <div className="error-state">{error}</div> : null}

      {!loading && smartLists.length > 0 ? (
        <section className="youtube-shelf">
          <h3>For you</h3>
          <div className="youtube-playlist-grid">
            {smartLists.map((list) => (
              <LibraryListCard key={list.id} list={list} />
            ))}
          </div>
        </section>
      ) : null}

      {!loading && userLists.length > 0 ? (
        <section className="youtube-shelf">
          <h3>Your lists</h3>
          <div className="youtube-playlist-grid">
            {userLists.map((list) => (
              <LibraryListCard key={list.id} list={list} />
            ))}
          </div>
        </section>
      ) : null}

      {!loading && sourceLists.length > 0 ? (
        <section className="youtube-shelf">
          <h3>Playlists &amp; channels</h3>
          <div className="youtube-playlist-grid">
            {sourceLists.map((list) => (
              <LibraryListCard key={list.id} list={list} />
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
                  onDeleted={(id) =>
                    setVideos((items) => items.filter((item) => item.id !== id))
                  }
                />
              ))}
            </div>
          )}
        </section>
      ) : null}
    </section>
  );
}
