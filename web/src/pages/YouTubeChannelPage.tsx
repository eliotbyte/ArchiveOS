import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import {
  ApiError,
  BROWSE_SORT_LABELS,
  type BrowseSort,
  type ChannelDetail,
  type EntityListItem,
  displayTitle,
} from "../api/client";
import PreviewImage from "../components/PreviewImage";
import YouTubeVideoCard from "../components/youtube/YouTubeCards";
import { useVideoPlayer } from "../context/VideoPlayerContext";
import { useVaultApi } from "../context/VaultContext";

export default function YouTubeChannelPage() {
  const { channelId = "" } = useParams();
  const api = useVaultApi();
  const { playEntity } = useVideoPlayer();
  const [channel, setChannel] = useState<ChannelDetail | null>(null);
  const [videos, setVideos] = useState<EntityListItem[]>([]);
  const [sort, setSort] = useState<BrowseSort>("published_desc");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!channelId) return;
    void (async () => {
      setLoading(true);
      setError(null);
      try {
        const [channelDetail, videoItems] = await Promise.all([
          api.getChannel(channelId),
          api.listChannelVideos(channelId, { sort, limit: 100 }),
        ]);
        setChannel(channelDetail);
        setVideos(videoItems);
      } catch (err) {
        setError(err instanceof ApiError ? err.message : "Failed to load channel");
        setChannel(null);
        setVideos([]);
      } finally {
        setLoading(false);
      }
    })();
  }, [api, channelId, sort]);

  const title = displayTitle(channel?.title, undefined, channelId);

  return (
    <section className="youtube-page youtube-channel-page">
      <div className="page-header">
        <Link className="secondary-link" to="/library/youtube">
          ← YouTube
        </Link>
      </div>

      {loading ? <p className="status-line">Loading channel…</p> : null}
      {error ? <p className="error">{error}</p> : null}

      {channel ? (
        <header className="yt-channel-header">
          {channel.avatar_preview ? (
            <div className="yt-channel-header-avatar">
              <PreviewImage
                preview={channel.avatar_preview}
                title={title}
                compact
              />
            </div>
          ) : null}
          <div className="yt-channel-header-copy">
            <h2>{title}</h2>
            {channel.follower_count ? (
              <p className="yt-channel-meta">
                {Number(channel.follower_count).toLocaleString()} subscribers
              </p>
            ) : null}
            {channel.description ? (
              <p className="yt-channel-description">{channel.description}</p>
            ) : null}
          </div>
        </header>
      ) : null}

      <div className="toolbar youtube-toolbar">
        <h3>Videos</h3>
        <label className="yt-sort-control inline">
          Sort
          <select
            value={sort}
            onChange={(event) => setSort(event.target.value as BrowseSort)}
          >
            <option value="published_desc">{BROWSE_SORT_LABELS.published_desc}</option>
            <option value="added_desc">{BROWSE_SORT_LABELS.added_desc}</option>
            <option value="views_desc">{BROWSE_SORT_LABELS.views_desc}</option>
          </select>
        </label>
      </div>

      {videos.length === 0 && !loading ? (
        <p className="status-line">No videos from this channel yet.</p>
      ) : (
        <div className="yt-video-grid">
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
  );
}
